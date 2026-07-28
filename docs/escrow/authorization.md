# Authorization Model and Invariants

This document is the authoritative reference for the escrow contract's authorization model. It covers every principal role, the per-entrypoint authorization rules, the four `ReleaseAuthorization` modes and their approval lifecycle, the invariants the model upholds, and a worked end-to-end example an auditor can follow.

**Source files:** `contracts/escrow/src/types.rs`, `contracts/escrow/src/approvals.rs`, `contracts/escrow/src/release.rs`, `contracts/escrow/src/deposit.rs`, `contracts/escrow/src/finalize.rs`, `contracts/escrow/src/create_contract.rs`, `contracts/escrow/src/governance.rs`, `contracts/escrow/src/lib.rs`

---

## 1. Principal Roles

The contract recognizes four principal roles. Each maps to a stored address on the `Contract` struct or on the global admin slot.

| Role | Storage field | Scope | Notes |
|------|--------------|-------|-------|
| **Admin** | `DataKey::Admin` (persistent) | Protocol-wide | Set once by `initialize`; can be rotated via a two-step timelock |
| **Client** | `Contract.client` | Per-contract | Set at `create_contract`; may change via `propose_client_migration` / `accept_client_migration` |
| **Freelancer** | `Contract.freelancer` | Per-contract | Set at `create_contract`; immutable |
| **Arbiter** | `Contract.arbiter` (optional) | Per-contract | Required for `ArbiterOnly` and `ClientAndArbiter` modes; must be distinct from client and freelancer |

The arbiter field is `Option<Address>`. An absent arbiter means the contract cannot use arbiter-gated authorization modes. Attempting to create a contract with `ArbiterOnly` or `ClientAndArbiter` mode without providing an arbiter panics with `MissingArbiter`.

---

## 2. Entrypoint Authorization Table

Every state-changing entrypoint that can move funds or mutate contract state is listed below. "Required signer" is the address the Soroban host checks via `require_auth()`. Entrypoints not listed here are read-only and require no auth.

### Admin-gated entrypoints

All of these require the address stored under `DataKey::Admin` to have authorized the call.

| Entrypoint | Required signer | Additional preconditions |
|-----------|----------------|--------------------------|
| `initialize(admin)` | `admin` (the passed argument) | Fails with `AlreadyInitialized` if already run |
| `bind_settlement_token(admin, token)` | `admin` == stored admin | Contract must be initialized; token must not already be bound; token must pass SAC probe |
| `pause()` | Stored admin | Contract must be initialized |
| `unpause()` | Stored admin | Contract must be initialized; blocked while emergency flag is set |
| `activate_emergency_pause()` | Stored admin | Contract must be initialized |
| `resolve_emergency()` | Stored admin | Contract must be initialized |
| `set_protocol_fee_bps(new_bps)` | Stored admin | Contract initialized; `new_bps ≤ 10_000` |
| `set_governed_params(admin, fee_bps, max_stroops)` | `admin` == stored admin | Contract initialized; `fee_bps ≤ 10_000` |
| `propose_governance_admin(proposed)` | Stored admin | Contract initialized |
| `accept_governance_admin()` | The pending proposed admin | Timelock of `ADMIN_ROTATION_MIN_DELAY_LEDGERS` (≈ 2 days) must have elapsed since `propose_governance_admin` |
| `cancel_governance_admin_proposal()` | Stored admin | A pending proposal must exist |
| `withdraw_protocol_fees(admin, amount)` | Stored admin | Settlement token must be bound; `AccumulatedProtocolFees ≥ amount` |

### Contract-lifecycle entrypoints

| Entrypoint | Required signer | Authorized role(s) | Additional preconditions |
|-----------|----------------|--------------------|--------------------------|
| `create_contract(client, freelancer, arbiter, milestones, mode)` | `client` | Client | Contract not paused; participants valid; milestones valid |
| `deposit_funds(contract_id, caller, amount)` | `caller` == `contract.client` | Client only | Contract initialized, not paused; status `Created` or `PartiallyFunded`; amount ≤ remaining unfunded total |
| `approve_milestone_release(contract_id, caller, milestone_index)` | `caller` | Mode-dependent (see §3) | Contract not paused, not finalized; status `Funded` or `PartiallyFunded`; milestone not released |
| `release_milestone(contract_id, caller, milestone_index)` | `caller` | Mode-dependent (see §3) | Contract not paused, not finalized; status `Funded`; valid non-expired approvals present |
| `refund_unreleased_milestones(contract_id, milestone_indices)` | `contract.client` | Client only | Contract not paused, not finalized; status `Created`, `Funded`, or `Disputed`; milestones meet deadline/overdue rules |
| `cancel_contract(contract_id, client)` | `client` == `contract.client` | Client only | Contract not paused, not finalized; status `Created` or `Funded`; `released_amount == 0` |
| `finalize_contract(contract_id, finalizer)` | `finalizer` | Client, freelancer, or arbiter | Contract not paused; status `Completed` or `Disputed`; not already finalized |
| `issue_reputation(contract_id, caller, rating, comment)` | `caller` == `contract.client` | Client only | Contract not paused; status `Completed`; reputation not yet issued; rating in [1,5] |
| `propose_client_migration(contract_id, current_client, new_client)` | `current_client` == `contract.client` | Current client | Contract not paused |
| `accept_client_migration(contract_id, new_client)` | `new_client` | Proposed new client | Contract not paused; live pending migration must exist |
| `resolve_dispute(contract_id, arbiter, resolution)` | `arbiter` | Arbiter only | Contract not paused; status `Disputed`; arbiter must be set and match |

### Global gate applied before all state-changing entrypoints

Before any of the above entrypoints reads or mutates contract state, `require_not_paused` checks both the `Paused` flag and the `Emergency` flag. If either is `true`, the call panics with `ContractPaused` or `EmergencyActive` respectively. This gate runs before `require_auth()` on the participant for lifecycle entrypoints, meaning a paused contract cannot be interacted with even by authorized principals.

---

## 3. ReleaseAuthorization — Data Model

```rust
// contracts/escrow/src/types.rs

pub enum ReleaseAuthorization {
    ClientOnly      = 0,
    ClientAndArbiter = 1,
    ArbiterOnly     = 2,
    MultiSig        = 3,
}

pub struct MilestoneApprovals {
    pub client_approved:    bool,
    pub freelancer_approved: bool,
    pub arbiter_approved:   bool,
}
```

`ReleaseAuthorization` is set once at `create_contract` and stored on `Contract.release_authorization`. It controls two independent checks for every milestone release:

1. **Approval gate** (`approve_milestone_release`): which principals may record approval.
2. **Release gate** (`release_milestone`): which principals may call the release entrypoint after approvals are satisfied.

### Mode matrix

| Mode | Arbiter required at creation | Who may approve | How many approvals needed | Who may call `release_milestone` |
|------|------------------------------|----------------|--------------------------|----------------------------------|
| `ClientOnly` | No | Client | 1 (client) | Client |
| `ClientAndArbiter` | **Yes** | Client or arbiter | 1 (either) | Client or arbiter |
| `ArbiterOnly` | **Yes** | Arbiter | 1 (arbiter) | Arbiter |
| `MultiSig` | No | Client and freelancer | 2 (both must approve) | Client **or** freelancer |

The `ClientAndArbiter` check uses OR logic: a single approval from either the client or the arbiter satisfies the check. This differs from `MultiSig`, which requires AND logic (both client and freelancer).

`MultiSig` separates approval (recording intent by each party) from release (executing the transfer). After both parties have approved, either party may call `release_milestone`. This prevents either side from holding the other hostage for the final on-chain transaction.

### Approval sufficiency logic

Implemented in `approvals::check_approvals` (`contracts/escrow/src/approvals.rs`):

```rust
match contract.release_authorization {
    ClientOnly       => approvals.client_approved,
    ArbiterOnly      => approvals.arbiter_approved,
    ClientAndArbiter => approvals.client_approved || approvals.arbiter_approved,
    MultiSig         => approvals.client_approved && approvals.freelancer_approved,
}
```

---

## 4. Approval Lifecycle

### 4.1 Storage

Approvals live in Soroban **temporary storage** under `DataKey::MilestoneApprovals(contract_id, milestone_index)`. Temporary storage entries are automatically evicted by the Soroban host when their TTL reaches zero.

| TTL constant | Ledgers | Wall time (≈5 s/ledger) |
|---|---|---|
| `PENDING_APPROVAL_TTL_LEDGERS` | 120,960 | ~7 days |
| `PENDING_APPROVAL_BUMP_THRESHOLD` | 17,280 | ~1 day |

When an approval is recorded, the entry TTL is set to `PENDING_APPROVAL_TTL_LEDGERS`. Each subsequent write resets it. If the entry is not accessed within the bump threshold of expiry, Soroban evicts it automatically.

### 4.2 Step-by-step flow

```
1. approve_milestone_release(contract_id, caller, milestone_index)
   ├─ require_not_paused, require_not_finalized
   ├─ caller.require_auth()
   ├─ load Contract, validate status (Funded or PartiallyFunded)
   ├─ validate milestone index, milestone not released
   ├─ verify caller role vs. mode (UnauthorizedRole if invalid)
   ├─ load or create MilestoneApprovals from temp storage
   ├─ check for duplicate approval (AlreadyApproved if duplicate)
   ├─ set caller's flag: client_approved / freelancer_approved / arbiter_approved
   └─ store with TTL = PENDING_APPROVAL_TTL_LEDGERS

2. release_milestone(contract_id, caller, milestone_index)
   ├─ require_not_paused
   ├─ caller.require_auth()
   ├─ load Contract, require_not_finalized
   ├─ validate status == Funded
   ├─ verify caller role vs. mode (UnauthorizedRole if invalid)
   ├─ load milestones, validate index, milestone not released or refunded
   ├─ check_approvals → reads temp storage; None / insufficient → InsufficientApprovals
   ├─ check available balance ≥ milestone.amount
   ├─ compute protocol_fee = floor(gross × fee_bps / 10_000)
   ├─ net_amount = gross_amount − protocol_fee
   ├─ SAC transfer: escrow → freelancer, amount = net_amount
   ├─ accumulate protocol_fee into AccumulatedProtocolFees
   ├─ mark milestone.released = true, update released_amount
   ├─ verify accounting invariant: released + refunded + accumulated_fees ≤ funded
   ├─ clear_approvals (remove temp storage entry)
   ├─ if all milestones released/refunded → status = Completed, grant reputation credit
   └─ emit events
```

### 4.3 Fail-closed properties

- A missing approval record (never set, or TTL expired) returns `None` from `env.storage().temporary().get(...)`, which `check_approvals` maps to `InsufficientApprovals`. The call panics without moving funds.
- Expired approvals are indistinguishable from absent approvals. Parties must re-approve if their approvals expire before the release is submitted.
- `clear_approvals` is called immediately after the SAC transfer succeeds, inside the same transaction. A partially executed transaction cannot leave stale approvals alive.

---

## 5. Authorization Invariants

The following invariants must hold at all times. Each is verified by reading the source code; the test evidence column references the test module that exercises the invariant.

### I1 — Admin is initialized before any money moves

`require_initialized` is called at the start of every money-flow entrypoint (`deposit_funds`, `release_milestone`, `cancel_contract`, `refund_unreleased_milestones`, `withdraw_protocol_fees`). Without initialization the admin slot is empty and safety rails (pause, fees) are unbound.

**Source:** `lib.rs::require_initialized`, called unconditionally in each entrypoint.
**Test:** `test/mainnet_readiness.rs`, `test/lifecycle.rs`

### I2 — Only the stored client may deposit

`deposit::validate_deposit` checks `caller != &contract.client` before anything else and panics with `UnauthorizedRole`. The client identity check runs before the SAC transfer so a rejected deposit cannot debit the caller.

**Source:** `deposit.rs::validate_deposit` line: `if caller != &contract.client`.

### I3 — Release callers are mode-restricted

`release_milestone` re-checks the caller's role against `contract.release_authorization` independently of `approve_milestone_release`. Even if approvals are present in storage, an unauthorized caller cannot trigger a release.

**Source:** `lib.rs::release_milestone` — the `match contract.release_authorization` block runs before `check_approvals`.

### I4 — Approvals are mode-restricted at record time

`approve_milestone_release` performs a role check before recording any approval. An arbiter cannot record a `client_approved = true` bit, and a freelancer cannot approve in `ClientOnly` or `ArbiterOnly` modes.

**Source:** `approvals.rs::approve_milestone` — the second `match contract.release_authorization` block.

### I5 — Approvals expire automatically

Approval records live in temporary storage. The Soroban host evicts them after `PENDING_APPROVAL_TTL_LEDGERS` (≈7 days) if not extended. Expired approvals cannot release funds.

**Source:** `ttl.rs` constants; `approvals.rs::approve_milestone` → `env.storage().temporary().extend_ttl(...)`.

### I6 — Approvals are consumed on release

`clear_approvals` is called unconditionally after a successful SAC transfer inside `release_milestone`. A given `(contract_id, milestone_index)` approval set can only be used once.

**Source:** `lib.rs::release_milestone` → `approvals::clear_approvals(...)`.

### I7 — Duplicate approvals are rejected

Each flag in `MilestoneApprovals` starts `false`. If the flag is already `true` when the same principal attempts to approve again, the call panics with `AlreadyApproved`.

**Source:** `approvals.rs::approve_milestone` — `if approvals.client_approved { return Err(AlreadyApproved); }` etc.

### I8 — ArbiterOnly and ClientAndArbiter require a non-null arbiter

`create_contract` panics with `MissingArbiter` if these modes are requested without an arbiter address. Arbiter is validated as distinct from client and freelancer (`InvalidArbiter`).

**Source:** `create_contract.rs` — the `match release_authorization` guard.

### I9 — Only client may cancel

`cancel_contract` verifies `client != contract.client → UnauthorizedRole` before `client.require_auth()`. Cancellation is additionally restricted to contracts with `released_amount == 0` and status `Created` or `Funded`.

**Source:** `lib.rs::cancel_contract`.

### I10 — Only the client may issue reputation

`issue_reputation` checks `caller != contract.client → UnauthorizedRole` before `caller.require_auth()`. Reputation can only be issued once per contract (`ReputationAlreadyIssued`), and only after status `Completed`.

**Source:** `lib.rs::issue_reputation`.

### I11 — Finalization is restricted to participants

`finalize_contract` calls `require_finalizer_role` which checks that the finalizer is the stored client, freelancer, or arbiter. Any other address panics with `UnauthorizedRole`.

**Source:** `finalize.rs::require_finalizer_role`.

### I12 — Admin rotation enforces a timelock

`accept_governance_admin` reads `pending.proposed_at_ledger` and computes `elapsed = current_ledger − proposed_at_ledger`. If `elapsed < ADMIN_ROTATION_MIN_DELAY_LEDGERS` (≈2 days), it panics with `TimelockNotElapsed`. Only after the delay may the pending admin call `accept_governance_admin` with their own `require_auth`.

**Source:** `governance.rs::accept_governance_admin_impl`.

### I13 — Pause gate runs before auth checks on lifecycle entrypoints

`require_not_paused` is the first instruction in every state-changing lifecycle entrypoint. This prevents paused contracts from being interacted with by any principal, including the admin (the admin uses a separate pause/unpause path).

**Source:** `lib.rs` — first line of `create_contract`, `deposit_funds`, `release_milestone`, `cancel_contract`, `refund_unreleased_milestones`, `issue_reputation`.

### I14 — Settlement token is write-once

`bind_settlement_token` checks `Self::read_settlement_token(&env).is_some()` before binding and panics with `SettlementTokenAlreadyBound` if a token is already present. This prevents substituting the custody token after contracts have been funded.

**Source:** `lib.rs::bind_settlement_token`.

### I15 — Accounting invariant is checked after every release

After each release, the contract verifies: `released_amount + refunded_amount + accumulated_fees ≤ funded_amount`. Violation panics with `AccountingInvariantViolated` and reverts the transaction.

**Source:** `lib.rs::release_milestone` — `if invariant_sum > contract.funded_amount { panic_with_error(AccountingInvariantViolated) }`.

---

## 6. Worked Example — MultiSig Two-Milestone Contract

This example traces the full authorization sequence for a contract with two milestones and `MultiSig` release mode.

### Setup

| Participant | Address |
|---|---|
| Client | `G...CLIENT` |
| Freelancer | `G...FREELANCER` |
| Arbiter | none (not required for MultiSig) |
| Mode | `MultiSig` |
| Milestones | 500 XLM (M0), 500 XLM (M1) |

### Step 1 — Admin initializes the contract

```
initialize(admin = G...ADMIN)
  → require_auth(G...ADMIN)
  → writes DataKey::Initialized = true, DataKey::Admin = G...ADMIN
```

### Step 2 — Admin binds settlement token and sets fee

```
bind_settlement_token(admin = G...ADMIN, token = G...USDC_SAC)
  → require_auth(G...ADMIN)
  → probes token::Client::balance(escrow_address) — must not panic
  → writes DataKey::SettlementToken = G...USDC_SAC

set_protocol_fee_bps(new_bps = 100)   // 1%
  → require_auth(G...ADMIN)
  → writes DataKey::ProtocolFeeBps = 100
```

### Step 3 — Client creates the escrow contract

```
create_contract(
  client      = G...CLIENT,
  freelancer  = G...FREELANCER,
  arbiter     = None,
  milestones  = [500_000_000, 500_000_000],  // stroops
  mode        = MultiSig
)
  → require_not_paused()
  → require_auth(G...CLIENT)
  → validates participants distinct, milestones valid, no arbiter required for MultiSig
  → writes DataKey::Contract(1), milestones vector
  → returns contract_id = 1
```

### Step 4 — Client deposits funds

```
deposit_funds(contract_id = 1, caller = G...CLIENT, amount = 1_000_000_000)
  → require_initialized(), require_not_paused()
  → validate_deposit: caller == contract.client ✓
  → SAC transfer: G...CLIENT → escrow, 1_000_000_000
  → apply_validated_deposit: require_auth(G...CLIENT)
  → funded_amount = 1_000_000_000, status = Funded
```

### Step 5 — Approve milestone 0

Both client and freelancer must approve (MultiSig mode).

```
approve_milestone_release(contract_id = 1, caller = G...CLIENT, milestone_index = 0)
  → require_not_paused(), require_not_finalized()
  → require_auth(G...CLIENT)
  → role check: is_client = true → allowed ✓
  → loads MilestoneApprovals{false, false, false} (absent → default)
  → sets client_approved = true
  → stores with TTL = 120,960 ledgers (~7 days)

approve_milestone_release(contract_id = 1, caller = G...FREELANCER, milestone_index = 0)
  → require_auth(G...FREELANCER)
  → role check: is_freelancer = true → allowed ✓
  → loads MilestoneApprovals{true, false, false}
  → sets freelancer_approved = true
  → stores updated record, resets TTL
```

### Step 6 — Release milestone 0

Either the client or freelancer may call `release_milestone` now that both have approved.

```
release_milestone(contract_id = 1, caller = G...CLIENT, milestone_index = 0)
  → require_not_paused()
  → require_auth(G...CLIENT)
  → status == Funded ✓
  → role check (MultiSig): is_client = true → allowed ✓
  → check_approvals: client_approved && freelancer_approved = true ✓
  → available = 1_000_000_000 − 0 − 0 = 1_000_000_000 ≥ 500_000_000 ✓
  → protocol_fee = floor(500_000_000 × 100 / 10_000) = 5_000_000
  → net_amount = 495_000_000
  → SAC transfer: escrow → G...FREELANCER, 495_000_000
  → AccumulatedProtocolFees += 5_000_000
  → milestone[0].released = true
  → released_amount = 495_000_000
  → invariant: 495_000_000 + 0 + 5_000_000 = 500_000_000 ≤ 1_000_000_000 ✓
  → clear_approvals(1, 0) — temp entry removed
  → not all milestones done; status remains Funded
```

### Step 7 — Attempt to re-approve milestone 0 (rejected)

```
approve_milestone_release(contract_id = 1, caller = G...CLIENT, milestone_index = 0)
  → milestone[0].released = true → MilestoneAlreadyReleased ✗
```

### Step 8 — Approve and release milestone 1

```
approve_milestone_release(1, G...CLIENT, 1)  → client_approved = true
approve_milestone_release(1, G...FREELANCER, 1) → freelancer_approved = true

release_milestone(1, G...FREELANCER, 1)
  → role check (MultiSig): is_freelancer = true → allowed ✓
  → check_approvals ✓
  → net_amount = 495_000_000
  → SAC transfer: escrow → G...FREELANCER, 495_000_000
  → all milestones done → status = Completed
  → PendingReputationCredits(G...FREELANCER) += 1
```

### Step 9 — Client issues reputation

```
issue_reputation(1, G...CLIENT, rating = 5, comment = "Excellent work")
  → require_auth(G...CLIENT)
  → caller == contract.client ✓, status == Completed ✓
  → reputation_issued = false → proceed
  → Reputation(G...FREELANCER).completed_contracts += 1, total_rating += 5
  → contract.reputation_issued = true
```

### Step 10 — Finalize the contract

```
finalize_contract(1, G...CLIENT)
  → require_auth(G...CLIENT)
  → require_finalizer_role: is_client = true ✓
  → status == Completed ✓
  → writes DataKey::Finalization(1) = FinalizationRecord{...}
```

After finalization, any further mutation (`deposit_funds`, `release_milestone`, `cancel_contract`, etc.) on contract 1 panics with `AlreadyFinalized`.

---

## 7. Error Quick-Reference

| Error | Code | Raised by |
|-------|------|-----------|
| `UnauthorizedRole` | 11 | Wrong caller role for the mode or operation |
| `AlreadyApproved` | 18 | Same party approving a milestone twice |
| `InsufficientApprovals` | 20 | Approvals absent, insufficient, or expired |
| `MissingArbiter` | 12 | `ArbiterOnly`/`ClientAndArbiter` mode without arbiter |
| `InvalidArbiter` | 13 | Arbiter equals client or freelancer |
| `AlreadyInitialized` | 34 | `initialize` called more than once |
| `NotInitialized` | 36 | Money-flow entrypoint before `initialize` |
| `ContractPaused` | 37 | Any state-changing call while paused |
| `EmergencyActive` | 38 | Any state-changing call during emergency |
| `AlreadyFinalized` | 46 | Mutation after finalization |
| `AlreadyCancelled` | 50 | `cancel_contract` on an already-cancelled contract |
| `TimelockNotElapsed` | 48 | Admin rotation accepted too soon |
| `SettlementTokenAlreadyBound` | (EscrowError::32) | Second `bind_settlement_token` call |
| `AccountingInvariantViolated` | 44 | Release causes `released + refunded + fees > funded` |
| `InvalidStatusTransition` | 41 | Operation invalid for current contract status |
| `ReputationAlreadyIssued` | 23 | `issue_reputation` called twice |

---

## 8. Cross-References

| Topic | Document |
|-------|---------|
| SAC token custody and transfer ordering | `docs/escrow/sac-custody.md` |
| Balance conservation invariant | `docs/escrow/balance-conservation-invariant.md` |
| Storage key schema and TTL policy | `docs/escrow/state-persistence.md`, `docs/escrow/storage-ttl.md` |
| Emergency controls | `docs/escrow/emergency-controls.md` |
| Protocol fee model | `docs/escrow/protocol-fees.md` |
| Dispute resolution | `docs/escrow/disputes.md` |
| Full ABI reference | `docs/escrow/abi-reference.md` |
| Security analysis | `docs/escrow/SECURITY.md` |
