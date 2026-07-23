# Escrow Data Model & Invariants

## Data Model

### Core State (`Contract`)

Stored under `DataKey::Contract(contract_id)` in persistent storage.

| Field | Type | Description |
|---|---|---|
| `client` | `Address` | Party funding the escrow. Authorises deposits, approvals, and cancellation. |
| `freelancer` | `Address` | Party performing the work. Receives milestone payouts. |
| `arbiter` | `Option<Address>` | Optional third-party resolver for disputes; required by `ArbiterOnly` / `ClientAndArbiter` auth modes. |
| `status` | `ContractStatus` | Current lifecycle state (see [State Machine](#state-machine)). |
| `total_deposited` | `i128` | Cumulative stroops ever deposited (increases monotonically). |
| `funded_amount` | `i128` | Stroops available for release/refund (equals `total_deposited`; tracked separately for accounting clarity). |
| `released_amount` | `i128` | Stroops paid out to freelancer (milestone releases + dispute freelancer awards). |
| `refunded_amount` | `i128` | Stroops returned to client (refunds + cancellation + dispute client awards). |
| `release_authorization` | `ReleaseAuthorization` | Who may approve/release milestones. |
| `reputation_issued` | `bool` | Whether the client has issued reputation for this contract. |

### Milestone State (`Milestone`)

Stored as a `Vec<Milestone>` under key `(DataKey::Contract(contract_id), Symbol("milestones"))`.

| Field | Type | Description |
|---|---|---|
| `amount` | `i128` | Payout value in stroops (set at creation, immutable). |
| `funded_amount` | `i128` | Reserved for per-milestone funding (currently always 0). |
| `released` | `bool` | Whether this milestone has been paid out. |
| `refunded` | `bool` | Whether this milestone has been refunded. |
| `work_evidence` | `Option<String>` | Freelancer-submitted deliverable reference (≤256 bytes). |
| `refunded_amount` | `i128` | Reserved for partial milestone refund tracking (currently unused). |
| `deadline` | `Option<u64>` | Optional Unix timestamp for timeout-based refunds. |

### Lifecycle States (`ContractStatus`)

```
Created  ──→ PartiallyFunded ──→ Funded ──→ Completed
  │             │                  │           │
  │             │                  ├──→ Disputed ──→ Completed / Refunded
  │             │                  │
  └──→ Cancelled                   └──→ Refunded
```

| State | Meaning | Mutating Entrypoints Accepted |
|---|---|---|
| `Created` (0) | Contract created, no deposit yet | `deposit_funds`, `cancel_contract` |
| `Accepted` (1) | Reserved | — |
| `Funded` (2) | Fully deposited, milestones may be released | `release_milestone`, `refund_unreleased_milestones`, `raise_dispute`, `cancel_contract`, `submit_work_evidence` |
| `PartiallyFunded` (7) | Partially deposited, awaiting more funds | `deposit_funds`, `raise_dispute`, `cancel_contract` |
| `Completed` (3) | All milestones terminal (released or refunded) | `finalize_contract`, `issue_reputation` |
| `Disputed` (4) | Dispute open, releases blocked | `resolve_dispute` |
| `Cancelled` (5) | Client cancelled before any release | *(terminal)* |
| `Refunded` (6) | All milestones refunded | *(terminal)* |

### Release Authorization Modes

| Mode | Approvers | Release Callers |
|---|---|---|
| `ClientOnly` (0) | Client | Client |
| `ClientAndArbiter` (1) | Client OR Arbiter | Client OR Arbiter |
| `ArbiterOnly` (2) | Arbiter | Arbiter |
| `MultiSig` (3) | Client AND Freelancer | Client OR Freelancer |

### Storage Key Schema

| Key | Type | Location | TTL |
|---|---|---|---|
| `Contract(id)` | `Contract` | Persistent | 30d (bumped on read/write) |
| `(Contract(id), "milestones")` | `Vec<Milestone>` | Persistent | 30d (bumped on read/write) |
| `MilestoneApprovals(id, idx)` | `MilestoneApprovals` | Temporary | ~7d |
| `MilestoneReleased(id, idx)` | `bool` | Persistent | 30d |
| `ReputationIssued(id)` | `bool` | Persistent | 30d |
| `Finalization(id)` | `FinalizationRecord` | Persistent | Indefinite |
| `PendingClientMigration(id)` | `PendingClientMigration` | Temporary | ~21d |
| `NextContractId` | `u32` | Persistent | 30d |
| `SettlementToken` | `Address` | Persistent | Indefinite |
| `Admin` | `Address` | Persistent | Indefinite |
| `Paused` | `bool` | Persistent | Indefinite |
| `Emergency` | `bool` | Persistent | Indefinite |
| `AccumulatedProtocolFees` | `i128` | Persistent | Indefinite |
| `GovernedParameters` | `GovernedParameters` | Persistent | Indefinite |

---

## Invariants

### I1: Accounting Invariant (Token Balance Conservation)

```
contract_token_balance == funded_amount - released_amount - refunded_amount + accumulated_protocol_fees
```

The on-chain SAC token balance held by the escrow contract must always equal the
derived accounting balance. Protocol fees are retained in-contract until
explicitly withdrawn. Every token movement is paired with the matching accounting
mutation within the same entrypoint (Checks-Effects-Interactions ordering).

*Violation would allow*: insolvency (contract cannot honour its obligations) or
locked funds.

*Enforced by*: `contracts/escrow/src/test/accounting_invariants.rs` after every
operation. The `assert_balance_conservation` helper cross-checks the on-chain
token balance against `funded_amount - released_amount - refunded_amount + accumulated_protocol_fees`.

### I2: Refundable Balance Invariant

```
refundable_balance = funded_amount - released_amount - refunded_amount
refundable_balance >= 0  at all times
```

The available balance is non-negative — the contract is never insolvent. When
`refundable_balance == 0` every milestone is terminal (released or refunded).

*Violation would allow*: over-payment (releasing or refunding more than was
funded), permanently locking funds.

*Enforced by*: every mutating entrypoint recomputes this value before any
transfer. `refund_unreleased_milestones` (refund_impl.rs:191) calls
`check_sufficient_balance`. `release_milestone` (release.rs:93-97) checks
`available_balance < milestone.amount`. `cancel_contract` (lib.rs:1622-1631)
computes refund from this value.

### I3: Dispute Conservation Invariant

```
released_amount + refunded_amount == funded_amount
```

After `resolve_dispute` completes, every stroop is accounted for exactly once.
The arbiter's resolution splits only the *available* balance
(`funded_amount - released_amount - refunded_amount`); pre-dispute releases are
preserved.

*Violation would allow*: value creation or destruction during dispute resolution.

*Enforced by*: `dispute.rs` line 39-41 panics with `AccountingInvariantViolated`
if `available < 0`. `Split` variant line 64 requires `total == available`. The
on-chain entrypoint at lib.rs:2296-2302 atomically adds both legs.

### I4: Status Consistency After Refund

After `refund_unreleased_milestones`:
- If **all** milestones are refunded → status = `Refunded`
- If **all** milestones are either released or refunded (mixed) → status = `Completed`
- Otherwise → status remains `Funded`

*Enforced by*: `refund_impl.rs:214-226` (`update_contract_status`).

### I5: Status Consistency After Release

After `release_milestone`:
- If all milestones are released or refunded → status = `Completed`, pending
  reputation credit granted.
- Otherwise → status remains `Funded`.

*Enforced by*: `release.rs:122-128`.

### I6: Status Consistency After Dispute

After `resolve_dispute`:
- If `refunded_amount == funded_amount` → status = `Refunded`
- Otherwise → status = `Completed`

*Enforced by*: `dispute.rs:76-82` (`final_status_after_resolution`).

### I7: Cumulative Amount Monotonicity

```
funded_amount  is non-decreasing (only increased by deposit)
released_amount is non-decreasing (only increased by release or dispute)
refunded_amount is non-decreasing (only increased by refund, cancel, or dispute)
total_deposited is non-decreasing (only increased by deposit)
```

No entrypoint decreases these accumulators. `total_deposited` and
`funded_amount` move together (one stroop deposited = one stroop funded).

### I8: Non-Negative Amounts

All amounts at all times satisfy:
```
amount > 0  for deposits, milestone amounts, refund amounts
funded_amount >= 0, released_amount >= 0, refunded_amount >= 0
```

*Enforced by*: `amount_validation.rs` (positive check on creation), deposit
validation (deposit.rs:25), refund validation (refund_impl.rs:182 — amounts are
from milestones which are already validated positive).

### I9: No Double-Spend Per Milestone

A milestone can be released XOR refunded, never both:

```
milestone.released && milestone.refunded  →  impossible
```

*Enforced by*: `release.rs:86-88` rejects `AlreadyRefunded` milestones;
`refund_impl.rs:173-179` rejects `AlreadyReleased` and `AlreadyRefunded`.

### I10: Terminal-State Immutability

Once a contract reaches `Cancelled` or `Refunded`, no value-moving operation is
permitted:

```
status ∈ {Cancelled, Refunded}  ⇒  deposit_funds, release_milestone, refund_unreleased_milestones, cancel_contract all rejected
```

*Enforced by*: deposit.rs:41-46, refund_impl.rs:92-97, lib.rs (cancel_contract
line 1612, release_milestone requires `Funded`).

### I11: Finalization Single-Write

`finalize_contract` writes exactly once per `contract_id`. Once
`DataKey::Finalization(contract_id)` exists, all contract-specific mutating
entrypoints reject with `AlreadyFinalized`.

*Enforced by*: `finalize.rs:42-46` (`require_not_finalized`), checked at the
top of `release_milestone`, `deposit_funds`, `cancel_contract`,
`refund_unreleased_milestones`, `submit_work_evidence`, `raise_dispute`,
`resolve_dispute`.

### I12: Cancel Restriction

`cancel_contract` is allowed only when `released_amount == 0` and status is
`Created` or `Funded`. This prevents cancellation after any milestone has been
paid out.

*Enforced by*: lib.rs:1612-1618.

### I13: Deposit Bounds

```
for ExactTotal mode:  deposit must equal total milestone sum exactly
for Incremental mode: deposit must not cause funded_amount > total milestone sum
```

*Enforced by*: deposit.rs:76 (`new_funded_amount > total_amount`).

### I14: Contract Creation Bounds

```
1 ≤ milestones.len() ≤ MAX_MILESTONES (10)
milestones[i].amount > 0 for all i
sum(milestones[i].amount) ≤ max_escrow_total_stroops (governed cap)
client != freelancer
arbiter != client and arbiter != freelancer (if present)
```

*Enforced by*: `create_contract.rs:57-113`, `amount_validation.rs`.

### I15: Approval-Required Release

For `ClientOnly`, `ClientAndArbiter`, `ArbiterOnly`:
`release_milestone` requires at least one matching approval in temporary storage
before payout.

For `MultiSig`: both `client_approved` and `freelancer_approved` must be true
before release.

Expired approvals (TTL ~7d) are treated as absent.

*Enforced by*: `approvals.rs:180-212` (`check_approvals`).

---

## Entrypoints That Touch the Model

### Mutating Entrypoints

| Entrypoint | Reads | Writes | Token Transfer |
|---|---|---|---|
| `create_contract` | `NextContractId`, `GovernedParameters` | `Contract(id)`, `(Contract(id), "milestones")`, `NextContractId` | No |
| `deposit_funds` | `Contract(id)`, `(Contract(id), "milestones")` | `Contract(id).funded_amount`, `Contract(id).total_deposited`, `Contract(id).status` | `client → escrow` |
| `release_milestone` | `Contract(id)`, `(Contract(id), "milestones")`, `MilestoneApprovals`, `AccumulatedProtocolFees` | milestone.released, `Contract(id).released_amount`, `Contract(id).status`, `AccumulatedProtocolFees`, `PendingReputationCredits` | `escrow → freelancer` (net of fee) |
| `refund_unreleased_milestones` | `Contract(id)`, `(Contract(id), "milestones")`, `SettlementToken` | milestone.refunded, `Contract(id).refunded_amount`, `Contract(id).status` | `escrow → client` |
| `cancel_contract` | `Contract(id)`, `SettlementToken` | `Contract(id).refunded_amount`, `Contract(id).status` | `escrow → client` |
| `raise_dispute` | `Contract(id)` | `Contract(id).status` | No |
| `resolve_dispute` | `Contract(id)` | `Contract(id).released_amount`, `Contract(id).refunded_amount`, `Contract(id).status`, `PendingReputationCredits` | No (accounting only; actual transfer is a separate step) |
| `approve_milestone_release` | `Contract(id)` | `MilestoneApprovals(id, idx)` (temporary) | No |
| `submit_work_evidence` | `Contract(id)`, `(Contract(id), "milestones")` | `milestone.work_evidence` | No |
| `finalize_contract` | `Contract(id)`, `(Contract(id), "milestones")` | `Finalization(id)` | No |
| `issue_reputation` | `Contract(id)`, `PendingReputationCredits`, `Reputation` | `Contract(id).reputation_issued`, `Reputation`, `ReputationIssued(id)`, `ReputationComment(id)`, `PendingReputationCredits` | No |
| `propose_client_migration` | `Contract(id)` | `PendingClientMigration(id)` (temporary) | No |
| `accept_client_migration` | `PendingClientMigration(id)` | `Contract(id).client` | No |

### Read-Only Entrypoints

| Entrypoint | Reads |
|---|---|
| `get_contract` | `Contract(id)` |
| `get_milestones` | `(Contract(id), "milestones")` |
| `get_milestone` | `(Contract(id), "milestones")` |
| `get_contract_summary` | `Contract(id)`, `(Contract(id), "milestones")` |
| `get_refundable_balance` | `Contract(id)` |
| `get_milestone_approvals` | `MilestoneApprovals(id, idx)` (temporary) |
| `get_finalization_record` | `Finalization(id)` |
| `get_reputation` | `Reputation(address)` |
| `get_average_rating` | `Reputation(address)` |
| `get_reputation_comment` | `ReputationComment(id)` |
| `get_pending_reputation_credits` | `PendingReputationCredits(address)` |
| `get_pending_client_migration` | `PendingClientMigration(id)` |
| `has_pending_client_migration` | `PendingClientMigration(id)` |
| `is_milestone_overdue` | `Contract(id)`, `(Contract(id), "milestones")` |

---

## Worked Example

### Scenario: 3-Milestone Freelance Contract

```rust
// Alice (client) hires Bob (freelancer) for 150 stroops across 3 milestones
// [50, 60, 40]. Alice controls releases (ClientOnly). No arbiter.

// ── Step 1: Create ─────────────────────────────────────────────────────
let id = escrow.create_contract(&alice, &bob, &None,
    &vec![&env, 50_i128, 60_i128, 40_i128],
    &ReleaseAuthorization::ClientOnly);
// contract:        status=Created, total_deposited=0, funded=0, released=0, refunded=0
// milestones:      [{amount:50, released:false, refunded:false}, ...]
// Account balance: 0 (no token bound yet)
// Check: I1 (balance==0-0-0+0), I2 (available==0), I14 bounds satisfied

// ── Step 2: Deposit full amount ────────────────────────────────────────
escrow.deposit_funds(&id, &alice, &150);
// contract:        status=Funded, total_deposited=150, funded=150, released=0, refunded=0
// milestones:      unchanged
// Token movement: 150 from alice → escrow
// Check: I1 (balance==150-0-0+0), I2 (available==150), I7 (funded increased), I8 (positive)

// ── Step 3: Release milestone 0 ────────────────────────────────────────
// Alice approves:
escrow.approve_milestone_release(&id, &alice, &0);
// Release:
escrow.release_milestone(&id, &alice, &0);
// milestones:      [{amount:50, released:true}, ...]
// contract:        status=Funded, released=50
// Token movement: 50 from escrow → bob (less protocol fee if set)
// Check: I2 (available==100), I5 (not all terminal), I7 (released increased), I9 (not both)

// ── Step 4: Release milestone 1 ────────────────────────────────────────
escrow.release_milestone(&id, &alice, &1);
// milestones:      [{..released:true}, {amount:60, released:true}, ...]
// contract:        status=Funded, released=110
// Check: I2 (available==40), I5 (not all terminal yet)

// ── Step 5: Alice discovers a bug in milestone 2 work, requests refund ─
escrow.refund_unreleased_milestones(&id, &vec![&env, 2_u32]);
// milestones[2]:   refunded=true
// contract:        status=Completed, released=110, refunded=40
// Token movement: 40 from escrow → alice
// Check: I1 (balance==150-110-40+0==0), I2 (available==0), I5 (all terminal → Completed)
//         I4 (mixed released+refunded → Completed), I9 (not both per milestone)

// ── Final state ──────────────────────────────────────────────────────
// Alice paid:   150 total (110 to Bob, 40 returned)
// Bob received: 110
// Escrow holds: 0
// Invariants:   all hold
```

### Cross-Reference: Invariant Coverage by Entrypoint

| Entrypoint | Invariants Exercised |
|---|---|
| `create_contract` | I8, I14 |
| `deposit_funds` | I1, I2, I7, I8, I10, I13 |
| `release_milestone` | I1, I2, I5, I7, I8, I9, I10, I11, I15 |
| `refund_unreleased_milestones` | I1, I2, I4, I7, I8, I9, I10, I11 |
| `cancel_contract` | I1, I2, I7, I8, I10, I11, I12 |
| `raise_dispute` | I10, I11 |
| `resolve_dispute` | I1, I3, I6, I7, I8, I10, I11 |

### Test Coverage

| Invariant | Test File(s) |
|---|---|
| I1 (token balance conservation) | `test/accounting_invariants.rs` |
| I2 (refundable balance) | `test/accounting_invariants.rs`, `test/refund.rs` |
| I3 (dispute conservation) | `test/dispute.rs` |
| I4 (status after refund) | `test/refund.rs` |
| I5 (status after release) | `test/release.rs` |
| I6 (status after dispute) | `test/dispute.rs` |
| I9 (no double-spend) | `test/accounting_invariants.rs`, `test/release.rs`, `test/refund.rs` |
| I10 (terminal-state) | `test/cancel_contract.rs`, `test/refund.rs` |
| I11 (finalization) | `test/persistence.rs` |
| I12 (cancel restriction) | `test/cancel_contract.rs` |
| I13 (deposit bounds) | `test/deposit.rs`, `test/input_sanitization_amounts.rs` |
| I14 (creation bounds) | `test/create_contract.rs`, `test/create_contract_bounds.rs` |
| I15 (approval matrix) | `test/release_authorization.rs` |
