# Contracts Data Model and Invariants

> Cross-references point to source files in `contracts/escrow/src/`.  
> Line numbers reflect the state at the time of writing; minor drift may occur.

---

## 1. Data Model

### 1.1 `Contract` — the core escrow entity

Defined in `types.rs:215-226`. Persisted under `DataKey::Contract(contract_id)`.

| Field | Type | Description |
|-------|------|-------------|
| `client` | `Address` | Party that funds the escrow |
| `freelancer` | `Address` | Party that delivers milestones and receives payouts |
| `arbiter` | `Option<Address>` | Optional third-party resolver for disputes |
| `status` | `ContractStatus` | Current lifecycle state |
| `total_deposited` | `i128` | Total stroops ever deposited (monotonic counter) |
| `funded_amount` | `i128` | Total stroops credited toward milestone obligations |
| `released_amount` | `i128` | Total net stroops paid to freelancer (gross − fees) |
| `refunded_amount` | `i128` | Total stroops returned to client |
| `release_authorization` | `ReleaseAuthorization` | Who can approve milestone releases |
| `reputation_issued` | `bool` | Whether the client has rated this contract |

All amounts are in **stroops** (the smallest unit on Stellar: 1 token = 10⁷ stroops).

### 1.2 `ContractStatus` — lifecycle states

Defined in `types.rs:198-210`. Discriminants are wire-stable.

| Variant | Code | Meaning |
|---------|------|---------|
| `Created` | 0 | Contract exists, awaiting funds |
| `Accepted` | 1 | Reserved; no entrypoint transitions into this state |
| `Funded` | 2 | Fully funded; milestones can be released |
| `Completed` | 3 | All milestones released or refunded |
| `Disputed` | 4 | Dispute raised, awaiting arbiter resolution |
| `Cancelled` | 5 | Client cancelled before any release |
| `Refunded` | 6 | All funds returned to client |
| `PartiallyFunded` | 7 | Some deposits received, not yet fully funded |

### 1.3 `Milestone` — per-deliverable state

Defined in `types.rs:229-241`. Stored as a `Vec<Milestone>` under `(DataKey::Contract(id), "milestones")`.

| Field | Type | Description |
|-------|------|-------------|
| `amount` | `i128` | Target amount for this milestone (gross) |
| `funded_amount` | `i128` | Amount recorded at release time |
| `released` | `bool` | Whether paid to freelancer |
| `refunded` | `bool` | Whether returned to client |
| `work_evidence` | `Option<String>` | Freelancer-submitted deliverable proof |
| `refunded_amount` | `i128` | Amount refunded for this milestone |
| `deadline` | `Option<u64>` | Unix timestamp for timeout-refund eligibility. `None` means no deadline |

### 1.4 `ReleaseAuthorization` — approval model

Defined in `types.rs:244-256`. Set at creation and immutable.

| Variant | Code | Who can release |
|---------|------|-----------------|
| `ClientOnly` | 0 | Client only |
| `ClientAndArbiter` | 1 | Client or arbiter |
| `ArbiterOnly` | 2 | Arbiter only |
| `MultiSig` | 3 | Client or freelancer after both have approved |

### 1.5 `MilestoneApprovals` — temporary approval tracking

Defined in `types.rs:260-266`. Stored in **temporary** storage under `DataKey::MilestoneApprovals(contract_id, milestone_index)` with TTL of 7 days (`lib.rs:557-566`).

| Field | Type |
|-------|------|
| `client_approved` | `bool` |
| `freelancer_approved` | `bool` |
| `arbiter_approved` | `bool` |

### 1.6 `DataKey` — storage key schema

Defined in `types.rs:59-93`. Every on-chain datum is keyed by one of these variants.

| Key | Purpose |
|-----|---------|
| `Initialized` | One-time setup flag |
| `Admin` | Admin address |
| `Paused`, `Emergency` | Circuit-breaker flags |
| `Contract(u32)` | Per-contract `Contract` struct |
| `NextContractId` | Monotonic ID counter |
| `MilestoneReleased(u32, u32)` | Per-milestone release flag |
| `MilestoneApprovals(u32, u32)` | Temporary approval state |
| `ReputationIssued(u32)` | One-per-contract guard |
| `PendingReputationCredits(Address)` | Unclaimed credits per freelancer |
| `Reputation(Address)` | `Reputation` aggregate per freelancer |
| `ReputationComment(u32)` | Client's rating comment |
| `PendingClientMigration(u32)` | Migration proposal (temporary) |
| `GovernanceAdmin` | Governance admin address |
| `PendingGovernanceAdmin` | Rotation proposal |
| `ProtocolParameters` | Deprecated |
| `ProtocolFeeBps` | Fee rate in basis points |
| `PendingAdmin` | Two-step admin transfer proposal |
| `AccumulatedProtocolFees` | Fee total retained in contract |
| `GovernedParameters` | `{ protocol_fee_bps, max_escrow_total_stroops }` |
| `ReadinessChecklist` | `{ initialized, governed_params_set, emergency_controls_enabled }` |
| `Finalization(u32)` | Immutable `FinalizationRecord` |
| `SettlementToken` | Bound SAC token address |

### 1.7 Auxiliary types

| Type | Defined in | Purpose |
|------|------------|---------|
| `ContractSummary` | `types.rs:17-32` | Indexer-facing read projection with `refundable_balance` |
| `MilestoneSummary` | `types.rs:10-15` | Lightweight per-milestone fields |
| `ContractBounds` | `types.rs:43-53` | Compile-time limits returned by `get_bounds()` |
| `Reputation` | `types.rs:318-324` | `{ completed_contracts, total_rating, last_rating }` |
| `DisputeSplit` | `types.rs:328-333` | `{ client_amount, freelancer_amount }` |
| `DisputeResolution` | `types.rs:337-344` | `FullRefund / PartialRefund / FullPayout / Split(DisputeSplit)` |
| `GovernedParameters` | `types.rs:300-304` | Admin-configurable `{ protocol_fee_bps, max_escrow_total_stroops }` |
| `PendingAdminProposal` | `types.rs:310-314` | Two-step admin rotation state |
| `ReadinessChecklist` | `types.rs:278-287` | `{ initialized, governed_params_set, emergency_controls_enabled }` |
| `FinalizationRecord` | `finalize.rs:14-22` | `{ finalizer, timestamp, summary }` |
| `PendingClientMigration` | `migration.rs:8-13` | `{ current_client, proposed_client, requested_at_ledger, expires_at_ledger }` |
| `ValidatedDeposit` | `deposit.rs:7-12` | Preflight deposit closure |

### 1.8 Constants

| Constant | Value | Module |
|----------|-------|--------|
| `MAX_MILESTONES` | `10` | `lib.rs:90` |
| `MAX_SINGLE_AMOUNT_STROOPS` | `1_000_000_0000000` (1M tokens) | `amount_validation.rs:15` |
| `MAX_TOTAL_ESCROW_STROOPS` | `1_000_000_0000000` | `lib.rs:92` |
| `STROOP_PRECISION` | `7` (decimal places) | `amount_validation.rs:12` |
| `MIN_POSITIVE_AMOUNT` | `1` (stroop) | `amount_validation.rs:18` |
| `CONTRACT_SUMMARY_SCHEMA_VERSION` | `1` | `types.rs:6` |
| `LEDGERS_PER_DAY` | `17_280` | `ttl.rs:10` |
| `PENDING_APPROVAL_TTL_LEDGERS` | `120_960` (~7 days) | `ttl.rs:11` |
| `PENDING_MIGRATION_TTL_LEDGERS` | `362_880` (~21 days) | `ttl.rs:12` |
| `PERSISTENT_TTL_LEDGERS` | `518_400` (~30 days) | `ttl.rs:13` |
| `ADMIN_ROTATION_MIN_DELAY_LEDGERS` | `34_560` (~2 days) | `ttl.rs:15` |

Runtime-configurable: `GovernedParameters.max_escrow_total_stroops` overrides the `MAX_TOTAL_ESCROW_STROOPS` cap; falls back to `i128::MAX` when unset (`create_contract.rs:89-94`).

---

## 2. Accounting Invariants

Every stroop that enters the escrow is tracked by deterministic, monotonic counters. No value is created or destroyed.

### 2.1 Balance conservation

```
contract_token_balance == funded_amount
                        - released_amount
                        - refunded_amount
                        + accumulated_protocol_fees
```

The contract's actual SAC token balance must always equal the derived accounting book. Tested in `test/accounting_invariants.rs`.

### 2.2 Invariant enforced at every release

`lib.rs:870-873`

```rust
let invariant_sum = contract.released_amount + contract.refunded_amount + new_accumulated;
if invariant_sum > contract.funded_amount {
    env.panic_with_error(EscrowError::AccountingInvariantViolated);
}
```

`new_accumulated` = previously accumulated fees + the fee for this release. The invariant is checked **after** updating `released_amount` and `AccumulatedProtocolFees` but **before** writing the contract back to storage.

### 2.3 Dispute conservation

`dispute.rs:34-70` — `resolution_payouts` computes payouts over:

```
available = funded_amount - released_amount - refunded_amount
```

- `available < 0` → `AccountingInvariantViolated` (corrupted state)
- `Split(a, b)` requires `a >= 0`, `b >= 0`, `a + b == available`

The payout is applied atomically in `resolve_dispute` (`lib.rs:2301-2302`):
```rust
contract.refunded_amount += client_payout;
contract.released_amount += freelancer_payout;
```

After resolution: `released_amount + refunded_amount == funded_amount` holds.

### 2.4 Cancellation invariant

`lib.rs:1616-1617` — cancellation is only allowed when `released_amount == 0`. A contract with released milestones cannot be cancelled.

### 2.5 Deposit guard

`deposit.rs:76-78` — `new_funded_amount > total_amount` is rejected with `InvalidDepositAmount`. The sum of deposits never exceeds the sum of milestone amounts.

### 2.6 Checked arithmetic

All accounting mutations use `checked_add`, `checked_sub`, `checked_mul`, `checked_div`. Any overflow panics with `PotentialOverflow` or `InsufficientFunds`. There are no unchecked operations on money values.

---

## 3. State Machine

### 3.1 Status transitions

```
Created ─────deposit─────► PartiallyFunded ──deposit──► Funded
  │                              │                        │
  │                              │                        ├── release ──► (all done?) → Completed
  │                              │                        │
  ├── cancel ──► Cancelled       │                        ├── refund ───► (all refunded?) → Refunded
  │                              │                        │                   or Completed
  │                              └── cancel ──► Cancelled │
  │                                                       ├── raise_dispute ──► Disputed
  └── cancel ──► Cancelled                                │
                                                          └── cancel ──► Cancelled
                                                              (no releases)


Disputed ──resolve_dispute──► Completed / Refunded

Completed ──finalize──► [FinalizationRecord written]
Disputed  ──finalize──► [FinalizationRecord written]
```

### 3.2 Per-entrypoint state guards

| Entrypoint | Allowed statuses |
|------------|-----------------|
| `deposit_funds` | `Created`, `PartiallyFunded` |
| `release_milestone` | `Funded` |
| `refund_unreleased_milestones` | `Created`, `Funded`, `Disputed` |
| `cancel_contract` | `Created`, `Funded` (and `released_amount == 0`) |
| `raise_dispute` | `Funded`, `PartiallyFunded` |
| `resolve_dispute` | `Disputed` |
| `finalize_contract` | `Completed`, `Disputed` |
| `issue_reputation` | `Completed` |
| `submit_work_evidence` | `Funded` |

Terminal states (`Cancelled`, `Refunded`, `Completed`) reject further deposits, releases, refunds, and cancellations.

### 3.3 Global guards

All mutating entrypoints run these checks:

1. **Initialization** — `require_initialized` (`lib.rs:2133-2142`): rejects all money-flow operations before `initialize()` is called
2. **Pause/Emergency** — `require_not_paused` (`finalize.rs:48-65`): blocks all mutating operations
3. **Finalization** — `require_not_finalized` (`finalize.rs:42-46`): blocks mutations after `finalize_contract`

---

## 4. Entrypoints

Entrypoints are defined in the `#[contractimpl] impl Escrow` block across `lib.rs`, `create_contract.rs`, `governance.rs`, and helper modules.

### 4.1 Setup

| Entrypoint | Auth | Error codes | Source |
|------------|------|-------------|--------|
| `initialize(admin)` | `admin` | `AlreadyInitialized` | `lib.rs:~315` |
| `bind_settlement_token(admin, token)` | `admin` | `NotInitialized`, `AlreadyInitialized`, `SettlementTokenAlreadyBound`, `SettlementTokenIsSelf`, `SettlementTokenIsAdmin` | `lib.rs:~470` |
| `get_settlement_token()` | none | — | `lib.rs:~530` |

`initialize` is one-time. `bind_settlement_token` is write-once and performs a pre-bind balance probe against the SAC interface. Both require the stored admin's authorization.

### 4.2 Lifecycle

| Entrypoint | Auth | Guards | Source |
|------------|------|--------|--------|
| `create_contract(client, freelancer, arbiter, milestones, release_authorization)` | `client` | pause, max 10 milestones, arbiter required for `ArbiterOnly`/`ClientAndArbiter`, client ≠ freelancer, arbiter distinct, total ≤ governed cap | `create_contract.rs:41` |
| `deposit_funds(contract_id, caller, amount)` | `caller` must be stored client | pause, `Created`/`PartiallyFunded` only, amount > 0, `funded_amount` ≤ milestone total | `deposit.rs:104` |
| `release_milestone(contract_id, caller, milestone_index)` | `caller` per `release_authorization` | pause, not finalized, `Funded` only, milestone not released/refunded, approvals present, sufficient available balance (accounting for fees) | `lib.rs:690` |
| `refund_unreleased_milestones(contract_id, milestone_indices)` | stored client | pause, not finalized, `Created`/`Funded`/`Disputed` only, no duplicates, milestones not released/refunded, deadline overdue if set, sufficient balance | `lib.rs:1018` |
| `cancel_contract(contract_id, client)` | stored client | pause, not finalized, `Created`/`Funded` only, `released_amount == 0` | `lib.rs:1593` |
| `finalize_contract(contract_id, finalizer)` | `finalizer` must be client, freelancer, or arbiter | pause, not finalized, `Completed`/`Disputed` only | `finalize.rs:140` |

### 4.3 Dispute

| Entrypoint | Auth | Guards | Source |
|------------|------|--------|--------|
| `raise_dispute(contract_id, caller)` | `caller` must be client or freelancer | pause, `Funded`/`PartiallyFunded` only, arbiter must exist | `lib.rs:2184` |
| `resolve_dispute(contract_id, arbiter, resolution)` | stored arbiter | pause, initialized, not finalized, `Disputed` only, payout split must be valid | `lib.rs:2263` |

Dispute resolution uses the pure helper `dispute::resolution_payouts(contract, resolution)` (`dispute.rs:30-70`) and `dispute::final_status_after_resolution(contract)` (`dispute.rs:76-82`).

### 4.4 Reputation

| Entrypoint | Auth | Guards | Source |
|------------|------|--------|--------|
| `issue_reputation(contract_id, caller, rating, comment)` | `caller` must be stored client; `freelancer` must match stored freelancer | pause, not finalized, `Completed` only, rating ∈ [1,5], comment 1–200 bytes, not already issued | `lib.rs:~1680` |
| `get_reputation(address)` | none | — | `lib.rs:~1740` |
| `get_average_rating(address)` | none | returns rating × 10_000 (basis points) | `lib.rs:~1760` |
| `get_pending_reputation_credits(address)` | none | — | `lib.rs:~1780` |
| `submit_work_evidence(contract_id, caller, milestone_index, evidence)` | `caller` must be freelancer | pause, not finalized, `Funded` only | `lib.rs:~1350` |

### 4.5 Governance

| Entrypoint | Auth | Guards | Source |
|------------|------|--------|--------|
| `set_protocol_fee_bps(new_bps)` | stored admin | `new_bps ≤ 10_000` | `governance.rs:30` |
| `get_protocol_fee_bps()` | none | — | `governance.rs:50` |
| `set_governed_params(admin, protocol_fee_bps, max_escrow_total_stroops)` | stored admin | — | `governance.rs:70` |
| `get_governed_parameters()` | none | — | `governance.rs:100` |
| `pause()` | stored admin | not `Emergency` | `lib.rs:~1970` |
| `unpause()` | stored admin | not `Emergency` | `lib.rs:~1990` |
| `activate_emergency_pause()` | stored admin | — | `lib.rs:~2010` |
| `resolve_emergency()` | stored admin | — | `lib.rs:~2040` |
| `withdraw_protocol_fees(amount, to)` | stored admin | `amount > 0`, `amount ≤ accumulated_fees` | `lib.rs:~510` |

Two-step admin transfer: `propose_governance_admin_impl` / `accept_governance_admin_impl` with a 2-day timelock enforced at `governance.rs:180-220`.

### 4.6 Read-only queries

| Entrypoint | Source | Panics if |
|------------|--------|-----------|
| `get_contract(contract_id)` | `lib.rs:~1230` | `ContractNotFound` |
| `get_milestones(contract_id)` | `lib.rs:~1270` | `ContractNotFound` |
| `get_milestone(contract_id, index)` | `lib.rs:~1290` | returns `None` on unknown index |
| `get_contract_summary(contract_id)` | `lib.rs:~1300` | `ContractNotFound` |
| `get_refundable_balance(contract_id)` | `lib.rs:~1365` | `ContractNotFound` |
| `contract_exists(contract_id)` | `lib.rs:~1410` | never (returns `bool`) |
| `get_next_contract_id()` | `lib.rs:~1420` | never |
| `get_milestone_approvals(contract_id, index)` | `lib.rs:~1430` | returns `None` on unknown |
| `get_approval_deadline(contract_id, index)` | `lib.rs:~1440` | returns `None` on unknown |
| `is_milestone_overdue(contract_id, index)` | `lib.rs:~1460` | `ContractNotFound` |
| `get_admin()` | `lib.rs:~340` | never |
| `get_bounds()` | `lib.rs:~370` | never |
| `get_mainnet_readiness_info()` | `lib.rs:~390` | never |
| `get_finalization_record(contract_id)` | `finalize.rs:171` | never (returns `Option`) |
| `get_pending_admin_proposed_at()` | `lib.rs:~2150` | never |

### 4.7 Client migration

| Entrypoint | Auth | Guards | Source |
|------------|------|--------|--------|
| `propose_client_migration(contract_id, current_client, new_client)` | `current_client` | not `Completed`/`Cancelled`/`Refunded`/`Disputed`, 21-day TTL | `migration.rs:30` |
| `accept_client_migration(contract_id, new_client)` | `new_client` | proposal must exist and be live | `migration.rs:80` |
| `cancel_client_migration(contract_id, current_client)` | `current_client` | — | `migration.rs:120` |
| `has_pending_client_migration(contract_id)` | none | — | `migration.rs:140` |
| `get_pending_client_migration(contract_id)` | none | — | `migration.rs:150` |

---

## 5. Worked Example

Three-milestone escrow with deadlines and fees.

**Setup**: `initialize(admin)` + `bind_settlement_token(admin, USDC)` + `set_protocol_fee_bps(250)` (2.5 %). Admin has configured the protocol.

### Step 1 — create_contract

Call: `create_contract(client, freelancer, Some(arbiter), [100, 200, 300], "ClientAndArbiter")`

```
contract_id = 1
status      = Created
total_deposited = 0
funded_amount   = 0
released_amount = 0
refunded_amount = 0
```

Milestones (all `released=false`, `refunded=false`, `deadline=None`):

| Index | amount |
|-------|--------|
| 0 | 100 |
| 1 | 200 |
| 2 | 300 |

### Step 2 — deposit_funds

Client deposits 600 stroops (exact total).

```
status          = Funded       (funded_amount == total)
total_deposited = 600
funded_amount   = 600
```

Contract SAC token balance = 600 stroops. Accounting invariant holds:
`600 = 600 − 0 − 0 + 0`.

### Step 3 — release_milestone(0)

Client approves milestone 0 (via `approve_milestone_release`), then releases.

- `gross_amount = 100`
- `protocol_fee = floor(100 × 250 / 10000) = 2`
- `net_amount = 98` (transferred to freelancer)
- `fee = 2` (retained in contract)

```
released_amount   = 98
AccumulatedProtocolFees = 2
```

Accounting invariant: `98 + 0 + 2 ≤ 600` ✓  
Token balance check: `598 = 600 − 98 − 0 + 2` ✓

### Step 4 — refund_unreleased_milestones([2])

Milestone 2 had a deadline set and it's now overdue. Client calls `refund_unreleased_milestones([2])`.

- Validates: milestone 2 is not released, not refunded, deadline is overdue
- `refund_amount = 300`

```
refunded_amount = 300
```

Milestone 2: `refunded=true`, `refunded_amount=300`.  
Accounting invariant: `98 + 300 + 2 ≤ 600` ✓

### Step 5 — release_milestone(1)

Client approves milestone 1.

- `gross_amount = 200`
- `protocol_fee = floor(200 × 250 / 10000) = 5`
- `net_amount = 195`

```
released_amount   = 98 + 195 = 293
AccumulatedProtocolFees = 2 + 5 = 7
```

All milestones are now released or refunded → `status = Completed`.  
Freelancer receives `pendingReputationCredits += 1`.

Accounting invariant: `293 + 300 + 7 ≤ 600` ✓  
Token balance: `293 = 600 − 293 − 300 + 7` → client awaiting withdrawal: 0, freelancer received: 293, fees held: 7.

### Step 6 — issue_reputation

Client rates freelancer: `issue_reputation(contract_id, client, rating=5, comment="Excellent work")`.

- Validates: contract is `Completed`, freelancer matches, rating ∈ [1,5], comment 1–200 bytes, not already issued
- Decrements `pendingReputationCredits` from 1 to 0
- Updates `Reputation(freelancer)`: `completed_contracts=1`, `total_rating=5`, `last_rating=5`

### Step 7 — finalize_contract

Client or freelancer or arbiter calls `finalize_contract(contract_id, finalizer)`.

- Validates: status is `Completed` (or `Disputed`), finalizer is a participant
- Writes `FinalizationRecord { finalizer, timestamp, summary }` under `DataKey::Finalization(1)`
- After this point, all mutations on contract 1 are rejected with `AlreadyFinalized`

### Step 8 — withdraw_protocol_fees

Admin calls `withdraw_protocol_fees(amount=7, to=treasury)`.

- `AccumulatedProtocolFees` decreases from 7 to 0
- 7 stroops transferred from contract to treasury

Final token balance: `0 = 600 − 293 − 300 + 0 − 7` → books fully reconciled, no stranded funds.

---

## 6. Source Map

| Module | Lines | Role | Storage keys |
|--------|-------|------|--------------|
| `lib.rs` | 1–2327 | Contract wrapper, setup, money flows, reads, reputation, evidence, pause/emergency, fee withdrawal, dispute orchestration | `Initialized`, `Admin`, `SettlementToken`, `Paused`, `Emergency`, `ReadinessChecklist`, `Contract(id)`, milestone vector, `MilestoneApprovals`, `AccumulatedProtocolFees`, `ReputationIssued`, `PendingReputationCredits`, `Reputation`, `ReputationComment` |
| `types.rs` | 1–355 | Data types, error enums, `DataKey`, summaries, governance/dispute records | Declares key schema only; no direct storage access |
| `amount_validation.rs` | 1–408 | Stateless validation, checked arithmetic, deposit preflight | None; callers write validated amounts |
| `deposit.rs` | 1–145 | Deposit validation + accounting | `Contract(id)`, milestone vector |
| `approvals.rs` | 1–442 | Temporary approve/clear/check for milestone releases + authorization checks | `MilestoneApprovals(id, idx)` (temporary), reads `Contract(id)` and milestone vector |
| `dispute.rs` | 1–89 | Pure payout arithmetic, final-status selection | None; root entrypoints update `Contract(id)` |
| `finalize.rs` | 1–175 | `FinalizationRecord`, finalize guards, summary, `require_not_paused` | `Finalization(id)` |
| `migration.rs` | 1–169 | Client migration proposals, acceptance, cancellation | `PendingClientMigration(id)` (temporary), `Contract(id)` |
| `governance.rs` | 1–255 | Admin fee/param/rotation entrypoints | `Admin`, `ProtocolFeeBps`, `PendingAdmin`, `GovernedParameters`, `ReadinessChecklist` |
| `ttl.rs` | 1–199 | TTL constants, storage extend/read-if-live helpers | Extends caller-provided keys |
| `utils.rs` | 1–38 | `now_seconds()` helper | None |

---

## 7. Complementary Documentation

The `docs/escrow/` directory contains deeper dives into specific subsystems:

| Document | Covers |
|----------|--------|
| `balance-conservation-invariant.md` | Token ↔ accounting identity proof |
| `dispute-conservation-invariant.md` | Post-resolution conservation with examples |
| `sac-custody.md` | SAC token custody model |
| `protocol-fees.md` | Fee calculation, rounding, withdrawal |
| `status-transition-guardrails.md` | Detailed state machine |
| `authorization.md` / `access-control.md` | Role matrix |
| `storage-ttl.md` | TTL policies and eviction risk |
| `REFUND_IMPLEMENTATION.md` | Per-milestone refund flow |
| `FUNDING_ACCOUNTING.md` | Deposit accounting model |
| `REPUTATION.md` | Credit/rating system |
| `ERROR_CATALOG.md` | Complete error code reference |
| `abi-reference.md` | Public ABI listing |
| `contract-summary-schema-versioning.md` | Indexer schema evolution |
| `performance-baselines.md` | Gas benchmarks |
