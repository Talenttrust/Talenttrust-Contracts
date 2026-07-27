# Storage Authorization and Access Rules

This document describes **who may read or write each storage key**, **in which
contract state**, and **which errors** reject unauthorized or invalid storage
access. Every rule is verified against the source in
[`contracts/escrow/src/storage.rs`](../contracts/escrow/src/storage.rs),
[`contracts/escrow/src/finalize.rs`](../contracts/escrow/src/finalize.rs), and
each entrypoint module.

---

## 1. Roles

| Role | Identity source | Storage permissions |
|------|-----------------|---------------------|
| **Admin** | `DataKey::Admin` (set by `initialize`) | Read/write all governance keys (`Paused`, `Emergency`, `ProtocolFeeBps`, `GovernedParameters`, `PendingAdmin`, `AccumulatedProtocolFees`, `SettlementToken`, `ReadinessChecklist`). Never accesses per-contract storage directly. |
| **Client** | `Contract.client` | Read/write `Contract(id)`, `(Contract(id), "milestones")`, `MilestoneApprovals` (own flag), `ReputationIssued`, `Reputation`, `ReputationComment`, `PendingReputationCredits`. Initiates deposits, refunds, cancellations, and reputation issuance. |
| **Freelancer** | `Contract.freelancer` | Read `Contract(id)`, `(Contract(id), "milestones")`. Write `MilestoneApprovals` (own flag) in `MultiSig` mode. Write work evidence. Never initiates money movement except as co-signer in `MultiSig` release. |
| **Arbiter** | `Contract.arbiter` (`Option<Address>`) | Read `Contract(id)`, `(Contract(id), "milestones")`. Write `MilestoneApprovals` (own flag) in `ArbiterOnly`/`ClientAndArbiter` modes. Writes dispute resolution state via `resolve_dispute`. |
| **Anyone** | — | Read-only queries (`get_contract`, `get_milestones`, `get_milestone_approvals`, `get_reputation`, `get_average_rating`, etc.) never blocked by pause, emergency, or role checks. |

---

## 2. Global Storage Gates

Every storage-mutating entrypoint runs these guards **before** touching any
per-contract key:

| Order | Guard | Effect | Error if fails |
|-------|-------|--------|----------------|
| 1 | `require_initialized` | `DataKey::Initialized == true` | `NotInitialized` |
| 2 | `require_not_paused` | `DataKey::Paused == false` and `DataKey::Emergency == false` | `ContractPaused` / `EmergencyActive` |
| 3 | `caller.require_auth()` | Soroban signature verification | Soroban auth failure (no contract error) |
| 4 | `load_contract` → `ContractNotFound` | `DataKey::Contract(id)` present | `ContractNotFound` |
| 5 | `require_not_finalized` | `DataKey::Finalization(id)` absent | `AlreadyFinalized` |

Entrypoints for governance state (`set_protocol_fee_bps`, `pause`, `emergency`,
`withdraw_protocol_fees`, admin rotation) skip steps 4–5 because they operate on
global keys, not per-contract state. They authenticate the admin via
`DataKey::Admin` instead.

---

## 3. Per-Key Authorization Matrix

### 3.1 Global Governance Keys (`persistent`)

| Key | Who may read | Who may write | Relevant entrypoints |
|-----|-------------|---------------|---------------------|
| `DataKey::Initialized` | Anyone | `initialize` (admin, once) | `initialize` |
| `DataKey::Admin` | Anyone | `initialize`, `accept_governance_admin` | `initialize`, `accept_governance_admin` |
| `DataKey::Paused` | Anyone | Admin via `pause`, `unpause`, `activate_emergency_pause`, `resolve_emergency` | `pause`, `unpause`, `activate_emergency_pause`, `resolve_emergency` |
| `DataKey::Emergency` | Anyone | Admin via `activate_emergency_pause`, `resolve_emergency` | `activate_emergency_pause`, `resolve_emergency` |
| `DataKey::SettlementToken` | Anyone (read-only query) | Admin via `bind_settlement_token` (write-once) | `bind_settlement_token` |
| `DataKey::NextContractId` | Anyone (via `get_next_contract_id`) | `create_contract` (internal) | `create_contract` |
| `DataKey::ProtocolFeeBps` | Anyone | Admin via `set_protocol_fee_bps` | `set_protocol_fee_bps` |
| `DataKey::GovernedParameters` | Anyone | Admin via `set_governed_params` | `set_governed_params` |
| `DataKey::AccumulatedProtocolFees` | Anyone | `release_milestone` (increment), Admin via `withdraw_protocol_fees` (decrement) | `release_milestone`, `withdraw_protocol_fees` |
| `DataKey::PendingAdmin` | Anyone | Admin via `propose_governance_admin`, proposed admin via `accept_governance_admin`, admin via `cancel_governance_admin_proposal` | `propose_governance_admin`, `accept_governance_admin`, `cancel_governance_admin_proposal` |
| `DataKey::ReadinessChecklist` | Anyone (via `get_mainnet_readiness_info`) | `initialize`, `set_governed_params`, `activate_emergency_pause` | `initialize`, `set_governed_params`, `activate_emergency_pause` |

### 3.2 Per-Contract Keys (`persistent`)

| Key | Who may read | Who may write | Write entrypoints |
|-----|-------------|---------------|-------------------|
| `DataKey::Contract(id)` | Anyone (via `get_contract`) | Client, freelancer, or arbiter depending on operation | `create_contract` (create), `deposit_funds` (update), `release_milestone` (update), `refund_unreleased_milestones` (update), `cancel_contract` (update), `resolve_dispute` (update), `accept_client_migration` (update `client` field) |
| `(Contract(id), "milestones")` | Anyone (via `get_milestones`) | Same as `Contract(id)` | `create_contract` (create), `release_milestone` (update milestone flags), `refund_unreleased_milestones` (update milestone flags), `submit_work_evidence` (update `work_evidence` field) |
| `DataKey::Finalization(id)` | Anyone (via `get_finalization_record`) | Client, freelancer, or arbiter via `finalize_contract` (write-once); Admin via `rollback_contract` (remove) | `finalize_contract`, `rollback_contract` |
| `DataKey::ReputationIssued(id)` | Anyone | Client via `issue_reputation` (write-once per contract, flips to `true`) | `issue_reputation` |
| `DataKey::ReputationComment(id)` | Anyone (via `get_reputation_comment`) | Client via `issue_reputation` | `issue_reputation` |
| `DataKey::PendingReputationCredits(address)` | Anyone (via `get_pending_reputation_credits`) | `release_milestone` / `refund_unreleased_milestones` / `resolve_dispute` (increment), Client via `issue_reputation` (decrement) | `release_milestone`, `refund_unreleased_milestones`, `resolve_dispute`, `issue_reputation` |
| `DataKey::Reputation(address)` | Anyone (via `get_reputation`) | Client via `issue_reputation` | `issue_reputation` |

### 3.3 Temporary Storage Keys

| Key | Who may write | Who may read | TTL |
|-----|--------------|-------------|-----|
| `DataKey::MilestoneApprovals(id, index)` | Client, freelancer, or arbiter (per `ReleaseAuthorization` mode). Write via `approve_milestone_release`, revoke own flag via `revoke_approval`, clear by `release_milestone`. | Anyone (via `get_milestone_approvals`); `release_milestone` reads for approval check | 120 960 ledgers (~7 d), bump threshold 17 280 (~1 d) |
| `DataKey::PendingClientMigration(id)` | Current client via `propose_client_migration` (write), proposed client via `accept_client_migration` (remove), current client via `cancel_client_migration` (remove) | Anyone (via `get_pending_client_migration`); `accept_client_migration` and `cancel_client_migration` read to verify proposal | 362 880 ledgers (~21 d), bump threshold 51 840 (~3 d) |

---

## 4. Entrypoint → Storage Authorization Detail

### 4.1 `initialize`

```
Auth:    admin.require_auth()
Writes:  DataKey::Initialized = true
         DataKey::Admin = admin
         DataKey::NextContractId = 1
         DataKey::ReadinessChecklist.initialized = true
Panics:  AlreadyInitialized (if Initialized is already true)
```

### 4.2 `create_contract`

```
Auth:    client.require_auth()
Guards:  require_not_paused
Writes:  DataKey::Contract(id) ← new Contract
         (DataKey::Contract(id), "milestones") ← milestone vector
         DataKey::NextContractId += 1
Panics:  InvalidParticipant (client == freelancer)
         MissingArbiter (ArbiterOnly/ClientAndArbiter without arbiter)
         InvalidArbiter (arbiter == client or freelancer)
         EmptyMilestones, InvalidMilestoneAmount, TooManyMilestones
         TotalCapExceeded, ContractIdOverflow, ContractIdCollision
```

### 4.3 `bind_settlement_token`

```
Auth:    admin == DataKey::Admin, then admin.require_auth()
Guards:  require_initialized, require_not_paused
Writes:  DataKey::SettlementToken = token (write-once)
Panics:  SettlementTokenAlreadyBound, InvalidSettlementToken
         SettlementTokenIsSelf, SettlementTokenIsAdmin
```

### 4.4 `deposit_funds`

```
Auth:    caller == contract.client, then caller.require_auth()
Guards:  require_initialized, require_not_paused, require_not_finalized
Writes:  DataKey::Contract(id).funded_amount += amount
         DataKey::Contract(id).total_deposited += amount
         DataKey::Contract(id).status ← Funded or PartiallyFunded
Panics:  UnauthorizedRole, InvalidState, InvalidDepositAmount
         ContractCancelled, ContractRefunded, AmountMustBePositive
         SettlementTokenNotConfigured
State:   Created → Funded (full) or PartiallyFunded (partial)
         PartiallyFunded → Funded (full)
```

### 4.5 `approve_milestone_release`

```
Auth:    caller.require_auth(); then per ReleaseAuthorization mode:
         ClientOnly     → is_client
         ArbiterOnly    → is_arbiter
         ClientAndArbiter → is_client || is_arbiter
         MultiSig       → is_client || is_freelancer
Guards:  require_initialized, require_not_paused, require_not_finalized
Writes:  DataKey::MilestoneApprovals(id, index).{client,freelancer,arbiter}_approved = true (temporary, TTL)
Panics:  UnauthorizedRole, InvalidState (not Funded/PartiallyFunded)
         MilestoneAlreadyReleased, AlreadyApproved, IndexOutOfBounds
State:   Funded or PartiallyFunded only
```

### 4.6 `release_milestone`

```
Auth:    caller.require_auth(); then per ReleaseAuthorization mode:
         ClientOnly     → is_client
         ArbiterOnly    → is_arbiter
         ClientAndArbiter → is_client || is_arbiter
         MultiSig       → is_client || is_freelancer
Guards:  require_initialized, require_not_paused, require_not_finalized
Writes:  DataKey::Contract(id).released_amount += gross_amount
         DataKey::Contract(id).status ← Completed (if all milestones done)
         (Contract(id), "milestones")[index].released = true
         DataKey::MilestoneApprovals(id, index) ← cleared
         DataKey::AccumulatedProtocolFees += fee
         DataKey::PendingReputationCredits(freelancer) += 1 (if contract completes)
Panics:  UnauthorizedRole, InvalidState (not Funded)
         InsufficientApprovals, MilestoneAlreadyReleased
         AlreadyRefunded, InsufficientFunds, IndexOutOfBounds
State:   Funded only
```

### 4.7 `refund_unreleased_milestones`

```
Auth:    contract.client.require_auth()
Guards:  require_not_paused, require_not_finalized
Writes:  DataKey::Contract(id).refunded_amount += refund_amount
         (Contract(id), "milestones")[index].refunded = true
         DataKey::Contract(id).status ← Refunded (if all done) or Completed
Panics:  UnauthorizedRole, InvalidState, AlreadyReleased, AlreadyRefunded
         EmptyRefundRequest, DuplicateMilestoneInRefund
         IndexOutOfBounds, MilestoneNotOverdue, InsufficientFunds
State:   Created, Funded, or Disputed
```

### 4.8 `cancel_contract`

```
Auth:    contract.client.require_auth()
Guards:  require_not_paused, require_not_finalized
Writes:  DataKey::Contract(id).status = Cancelled
         (funds transferred back to client via SAC)
Panics:  UnauthorizedRole, InvalidStatusTransition, AlreadyCancelled
State:   Created or Funded (with released_amount == 0)
```

### 4.9 `raise_dispute`

```
Auth:    caller.require_auth(); caller must be client or freelancer
Guards:  require_initialized, require_not_paused, require_not_finalized
Writes:  DataKey::Contract(id).status = Disputed
Panics:  UnauthorizedRole, ArbiterRequired (arbiter is None)
         InvalidState (not Funded/PartiallyFunded)
State:   Funded or PartiallyFunded → Disputed
```

### 4.10 `resolve_dispute`

```
Auth:    arbiter.require_auth(); arbiter must match Contract.arbiter
Guards:  require_initialized, require_not_paused, require_not_finalized
Writes:  DataKey::Contract(id).released_amount / refunded_amount (adjusted)
         DataKey::Contract(id).status ← Completed or Refunded
         DataKey::PendingReputationCredits(freelancer) += 1 (if Completed)
Panics:  UnauthorizedRole, InvalidStatusTransition (not Disputed)
         InvalidDisputeSplit, AccountingInvariantViolated
         PotentialOverflow
State:   Disputed only
```

### 4.11 `finalize_contract`

```
Auth:    finalizer.require_auth(); must be client, freelancer, or arbiter
Guards:  require_not_paused, require_not_finalized
Writes:  DataKey::Finalization(id) ← FinalizationRecord (write-once)
Panics:  UnauthorizedRole, InvalidStatusTransition (not Completed/Disputed)
         AlreadyFinalized
State:   Completed or Disputed
```

### 4.12 `issue_reputation`

```
Auth:    caller.require_auth(); caller must be contract.client
Guards:  require_initialized, require_not_paused
Writes:  DataKey::ReputationIssued(id) = true
         DataKey::ReputationComment(id) = comment
         DataKey::Reputation(freelancer) ← updated counters
         DataKey::PendingReputationCredits(freelancer) -= 1
Panics:  UnauthorizedRole, InvalidRating, EmptyComment, CommentTooLong
         NotCompleted, ReputationAlreadyIssued, SelfRating
         InvalidState (no pending credit)
State:   Completed only (no finalization guard — reputation is post-close)
```

### 4.13 `propose_client_migration`

```
Auth:    current_client.require_auth(); must match contract.client
Guards:  require_not_paused, require_not_finalized
Writes:  DataKey::PendingClientMigration(id) ← proposal (temporary, TTL)
Panics:  UnauthorizedRole, InvalidState (already pending)
         InvalidStatusTransition (terminal states)
         InvalidParticipant (new == client or freelancer)
State:   Created, Accepted, Funded, or PartiallyFunded (not Completed, Cancelled, Refunded, Disputed)
```

### 4.14 `accept_client_migration`

```
Auth:    new_client.require_auth(); must match pending.proposed_client
Guards:  require_not_paused, require_not_finalized
Writes:  DataKey::Contract(id).client = new_client
         DataKey::PendingClientMigration(id) ← removed
Panics:  UnauthorizedRole, InvalidState (no pending migration)
         InvalidStatusTransition
State:   Same as propose
```

### 4.15 `submit_work_evidence`

```
Auth:    freelancer.require_auth(); must be contract.freelancer
Guards:  require_not_paused, require_not_finalized
Writes:  (Contract(id), "milestones")[index].work_evidence = evidence
Panics:  UnauthorizedRole, InvalidState (not Funded)
         MilestoneAlreadyReleased, AlreadyRefunded
         EvidenceTooLong (>256 bytes), IndexOutOfBounds
State:   Funded only
```

---

## 5. Storage Access and TTL

### 5.1 Persistent TTL extension

Every read or write of `DataKey::Contract(id)` and `(Contract(id), "milestones")`
triggers `extend_ttl(PERSISTENT_BUMP_THRESHOLD, PERSISTENT_TTL_LEDGERS)`:

| Key | Bump on read? | Bump on write? | Exception |
|-----|--------------|----------------|-----------|
| `DataKey::Contract(id)` | Yes (via `load_contract` and `extend_contract_ttl`) | Yes | `contract_exists` (pure `has()` probe, no bump) |
| `(Contract(id), "milestones")` | Yes (via `load_milestones`, `try_load_milestones`) | Yes (via `store_milestones`) | — |
| `DataKey::NextContractId` | No (via `get_next_contract_id`) | Yes (only from `create_contract`) | — |
| `DataKey::SettlementToken` | No | No | Intentionally not bumped (read-only) |
| `DataKey::Finalization(id)` | No | No (write-once) | — |

### 5.2 Temporary TTL extension

| Key | TTL | Bump threshold | Bump on read? |
|-----|-----|---------------|--------------|
| `DataKey::MilestoneApprovals(id, index)` | 120 960 ledgers (~7 d) | 17 280 ledgers (~1 d) | Yes, via `get_milestone_approvals` |
| `DataKey::PendingClientMigration(id)` | 362 880 ledgers (~21 d) | 51 840 ledgers (~3 d) | No (reads use `read_if_live` which does not bump) |

---

## 6. Rejection Summary (Storage-Related)

| Error | Code | When raised | Storage key context |
|-------|------|-------------|---------------------|
| `NotInitialized` | 36 | Any mutating entrypoint before `initialize` | `DataKey::Initialized` absent or `false` |
| `ContractPaused` | 37 | Any mutating entrypoint while `DataKey::Paused == true` | `DataKey::Paused` |
| `EmergencyActive` | 38 | Any mutating entrypoint while `DataKey::Emergency == true` | `DataKey::Emergency` |
| `AlreadyFinalized` | 46 | Any contract-specific mutation after `DataKey::Finalization(id)` written | `DataKey::Finalization(id)` |
| `ContractNotFound` | 10 | `DataKey::Contract(id)` absent from persistent storage | `DataKey::Contract(id)` |
| `AlreadyInitialized` | 34 | `initialize` called when `DataKey::Initialized` is already `true` | `DataKey::Initialized` |
| `SettlementTokenNotConfigured` | 52 | `deposit_funds` when `DataKey::SettlementToken` is absent | `DataKey::SettlementToken` |
| `SettlementTokenAlreadyBound` | — | `bind_settlement_token` when `DataKey::SettlementToken` is already present | `DataKey::SettlementToken` |
| `UnauthorizedRole` | 11 | Caller not authorized for the storage operation | Varies by entrypoint |
| `InvalidState` | 16 | Contract status not compatible with storage mutation | `DataKey::Contract(id).status` |
| `InsufficientApprovals` | 20 | `release_milestone` with missing/expired approvals | `DataKey::MilestoneApprovals(id, index)` |
| `AlreadyApproved` | 18 | Duplicate approval by same party | `DataKey::MilestoneApprovals(id, index)` |
| `MilestoneAlreadyReleased` | 17 | Approve/release/refund on `milestone.released == true` | `(Contract(id), "milestones")[i].released` |
| `AlreadyRefunded` | 8 | Release/refund on `milestone.refunded == true` | `(Contract(id), "milestones")[i].refunded` |
| `AlreadyReleased` | 9 | Refund of an already-released milestone | `(Contract(id), "milestones")[i].released` |
| `IndexOutOfBounds` | 3 | Milestone index ≥ vector length | `(Contract(id), "milestones")` |
| `ReputationAlreadyIssued` | 23 | `issue_reputation` when `DataKey::ReputationIssued(id)` is `true` | `DataKey::ReputationIssued(id)` |
| `NotCompleted` | 22 | `issue_reputation` when `Contract.status != Completed` | `DataKey::Contract(id).status` |
| `ArbiterRequired` | 42 | `raise_dispute` when `Contract.arbiter` is `None` | `DataKey::Contract(id).arbiter` |
| `InvalidStatusTransition` | 41 | State change not allowed by lifecycle | `DataKey::Contract(id).status` |
| `MissingArbiter` | 35 | `create_contract` with `ArbiterOnly`/`ClientAndArbiter` and no arbiter | — |
| `InvalidArbiter` | 36 | Arbiter equals client or freelancer at creation | — |
| `InsufficientFunds` | 9 | Available balance < milestone amount | `DataKey::Contract(id).funded_amount`, `.released_amount`, `.refunded_amount` |
| `AccountingInvariantViolated` | 44 | `available_balance` would become negative | `DataKey::Contract(id)` accounting fields |
| `PotentialOverflow` | 45 | Intermediate arithmetic overflow on storage values | `DataKey::Contract(id)` accounting fields |
| `AlreadyCancelled` | 50 | `cancel_contract` on already-cancelled contract | `DataKey::Contract(id).status` |
| `ContractCancelled` | 37 | `deposit_funds` on cancelled contract | `DataKey::Contract(id).status` |
| `ContractRefunded` | 38 | `deposit_funds` on refunded contract | `DataKey::Contract(id).status` |
| `EvidenceTooLong` | 47 | `submit_work_evidence` with >256 byte string | `(Contract(id), "milestones")[i].work_evidence` |
| `MilestoneNotOverdue` | 53 | `refund_unreleased_milestones` on milestone with future deadline | `(Contract(id), "milestones")[i].deadline` |
| `RollbackNotAllowed` | 54 | `rollback_contract` on non-finalized or wrong-status contract | `DataKey::Finalization(id)` + `DataKey::Contract(id).status` |
| `InvalidDisputeSplit` | 43 | `resolve_dispute` with amounts that don't conserve balance | `DataKey::Contract(id)` accounting fields |
| `InvalidRating` | 19 | Rating outside [1,5] in `issue_reputation` | — |
| `EmptyComment` | 29 | `issue_reputation` with empty comment | — |
| `CommentTooLong` | 30 | `issue_reputation` with comment >200 bytes | — |
| `SelfRating` | 39 | `issue_reputation` when client == freelancer | — |
| `EmptyRefundRequest` | 6 | `refund_unreleased_milestones` with empty index list | — |
| `DuplicateMilestoneInRefund` | 7 | Duplicate indices in `refund_unreleased_milestones` call | — |
| `AmountMustBePositive` | 15 | Deposit amount ≤ 0 | — |
| `InvalidDepositAmount` | 32 | Deposit would exceed total milestone sum | `DataKey::Contract(id).funded_amount` |
| `InvalidParticipant` | 31 | Client == freelancer at creation | — |
| `EmptyMilestones` | 25 | No milestones provided at creation | — |
| `InvalidMilestoneAmount` | 26 | Milestone amount ≤ 0 | — |
| `TooManyMilestones` | 34 | > MAX_MILESTONES milestones | — |
| `TotalCapExceeded` | 33 | Sum of milestones exceeds governed cap | `DataKey::GovernedParameters.max_escrow_total_stroops` |
| `ContractIdOverflow` | 28 | `NextContractId` would exceed `u32::MAX` | `DataKey::NextContractId` |
| `ContractIdCollision` | 27 | Allocated ID slot already occupied | `DataKey::Contract(id)` |
| `FreelancerMismatch` | 23 | Work evidence caller not freelancer | — |
| `TimelockNotElapsed` | 48 | `accept_governance_admin` before min delay | `DataKey::PendingAdmin.proposed_at_ledger` |
| `InvalidProtocolParameters` | 49 | Fee > 100% or invalid governed params | — |
| `EscrowCapExceeded` | 51 | Operation would exceed escrow cap | `DataKey::GovernedParameters.max_escrow_total_stroops` |
| `InsufficientAccumulatedFees` | 35 | `withdraw_protocol_fees` when accumulator is 0 | `DataKey::AccumulatedProtocolFees` |

---

## 7. Worked Example: ClientOnly Mode with Full Lifecycle

This example traces every storage key touched across a complete escrow lifecycle.

### Setup

```
admin     = GADM…
client    = GA…
freelancer = GB…
arbiter   = None
milestones = [5_000_000, 3_000_000] stroops
release_authorization = ClientOnly
```

### Step 1 — Initialize

```
initialize(admin = GADM…)
```

Storage writes:
- `DataKey::Initialized = true`
- `DataKey::Admin = GADM…`
- `DataKey::NextContractId = 1`
- `DataKey::ReadinessChecklist.initialized = true`

Who may call: **Admin only.** `admin.require_auth()`.

### Step 2 — Bind settlement token

```
bind_settlement_token(admin = GADM…, token = CASM…)
```

Storage writes:
- `DataKey::SettlementToken = CASM…`

Who may call: **Admin only.** `admin.require_auth()`. Write-once: second call → `SettlementTokenAlreadyBound`.

### Step 3 — Create contract

```
create_contract(client = GA…, freelancer = GB…, arbiter = None,
                milestones = [5_000_000, 3_000_000],
                release_authorization = ClientOnly)
```

Storage writes:
- `DataKey::Contract(1)`: `{client: GA…, freelancer: GB…, arbiter: None, status: Created, funded_amount: 0, ...}`
- `(DataKey::Contract(1), "milestones")`: `[{amount: 5_000_000, released: false, refunded: false}, {amount: 3_000_000, released: false, refunded: false}]`
- `DataKey::NextContractId = 2`

Who may call: **Client only.** `client.require_auth()`.

Storage reads:
- `DataKey::GovernedParameters` (to enforce cap)
- `DataKey::NextContractId` (for allocation)

### Step 4 — Deposit funds

```
deposit_funds(contract_id = 1, caller = GA…, amount = 8_000_000)
```

Storage writes:
- `DataKey::Contract(1).funded_amount = 8_000_000`
- `DataKey::Contract(1).total_deposited = 8_000_000`
- `DataKey::Contract(1).status = Funded`

Who may call: **Client only** (`caller == contract.client`). `caller.require_auth()`.

Guards: `require_initialized`, `require_not_paused`, `require_not_finalized`.

Rejected if:
- Status is `Cancelled` → `ContractCancelled`
- Status is `Refunded` → `ContractRefunded`
- Status is not `Created`/`PartiallyFunded` → `InvalidState`
- `DataKey::SettlementToken` absent → `SettlementTokenNotConfigured`

### Step 5 — Approve milestone 0

```
approve_milestone_release(contract_id = 1, caller = GA…, milestone_index = 0)
```

Storage writes:
- `DataKey::MilestoneApprovals(1, 0).client_approved = true` (temporary, TTL ~7 d)

Who may call: **Client only** for `ClientOnly` mode. `caller.require_auth()`.

Rejected if:
- Status not `Funded`/`PartiallyFunded` → `InvalidState`
- Milestone already released → `MilestoneAlreadyReleased`
- Already approved → `AlreadyApproved`

### Step 6 — Release milestone 0

```
release_milestone(contract_id = 1, caller = GA…, milestone_index = 0)
```

Storage writes:
- `DataKey::Contract(1).released_amount += 5_000_000`
- `(DataKey::Contract(1), "milestones")[0].released = true`
- `DataKey::MilestoneApprovals(1, 0)` ← cleared
- `DataKey::AccumulatedProtocolFees += fee`

Who may call: **Client only** for `ClientOnly` mode. `caller.require_auth()`.

Rejected if:
- Status not `Funded` → `InvalidState`
- Approvals absent/expired → `InsufficientApprovals`
- Milestone already released → `MilestoneAlreadyReleased`
- Insufficient balance → `InsufficientFunds`

After release: `released_amount = 5_000_000`, 2 milestones remain → status stays `Funded`.

### Step 7 — Approve and release milestone 1

Same pattern as steps 5–6. After release of milestone 1:

- `released_amount = 8_000_000`
- All milestones released → `status = Completed`
- `DataKey::PendingReputationCredits(GB…) += 1` (credit granted)

### Step 8 — Issue reputation

```
issue_reputation(contract_id = 1, caller = GA…, rating = 5, comment = "Excellent work")
```

Storage writes:
- `DataKey::ReputationIssued(1) = true` (write-once)
- `DataKey::ReputationComment(1) = "Excellent work"`
- `DataKey::Reputation(GB…).completed_contracts += 1`
- `DataKey::Reputation(GB…).total_rating += 5`
- `DataKey::Reputation(GB…).last_rating = 5`
- `DataKey::PendingReputationCredits(GB…) -= 1`

Who may call: **Client only.** `caller.require_auth()`.

Not gated by finalization (reputation is post-close metadata).

### Step 9 — Finalize

```
finalize_contract(contract_id = 1, finalizer = GA…)
```

Storage writes:
- `DataKey::Finalization(1)` ← `FinalizationRecord` (immutable snapshot)

Who may call: **Client, freelancer, or arbiter.** `finalizer.require_auth()`.

After finalization: all entrypoints that mutate per-contract state → `AlreadyFinalized`.

### Step 10 — Verify immutability

```
deposit_funds(contract_id = 1, caller = GA…, amount = 1_000_000)
→ AlreadyFinalized

release_milestone(contract_id = 1, caller = GA…, milestone_index = 0)
→ AlreadyFinalized
```

All per-contract storage mutations are permanently blocked. Reads remain available.

---

## 8. Source Cross-Reference

| Concern | Source file | Key lines |
|---------|------------|-----------|
| `require_initialized` | `contracts/escrow/src/storage.rs` | L24–L32 |
| `require_not_paused` | `contracts/escrow/src/storage.rs` | L127–L145 |
| `require_not_finalized` | `contracts/escrow/src/storage.rs` | L172–L177 |
| `load_contract` | `contracts/escrow/src/storage.rs` | L48–L53 |
| `load_milestones` | `contracts/escrow/src/storage.rs` | L69–L75 |
| `load_contract_checked` | `contracts/escrow/src/storage.rs` | L97–L114 |
| `DataKey` enum | `contracts/escrow/src/types.rs` | L202–L249 |
| `Error` enum | `contracts/escrow/src/types.rs` | L252–L310 |
| `EscrowError` enum | `contracts/escrow/src/lib.rs` | L142–L200 |
| `initialize` | `contracts/escrow/src/lib.rs` | L554–L588 |
| `create_contract` | `contracts/escrow/src/create_contract.rs` | L49–L266 |
| `bind_settlement_token` | `contracts/escrow/src/lib.rs` | L388–L444 |
| `deposit_funds` | `contracts/escrow/src/lib.rs` | L732–L745 |
| `deposit::validate_deposit` | `contracts/escrow/src/deposit.rs` | L20–L78 |
| `deposit::apply_validated_deposit` | `contracts/escrow/src/deposit.rs` | L102–L146 |
| `approve_milestone_release` → `approve_milestone` | `contracts/escrow/src/approvals.rs` | L52–L133 |
| `release_milestone` | `contracts/escrow/src/release.rs` | L75–L200 |
| `refund_unreleased_milestones` | `contracts/escrow/src/refund.rs` | L35–L130 |
| `cancel_contract` | `contracts/escrow/src/lib.rs` | L1593–L1651 |
| `raise_dispute` | `contracts/escrow/src/dispute.rs` | L312–L426 |
| `resolve_dispute` | `contracts/escrow/src/dispute.rs` | L366–L426 |
| `finalize_contract` | `contracts/escrow/src/finalize.rs` | L144–L176 |
| `issue_reputation` | `contracts/escrow/src/lib.rs` | L1739–L1838 |
| `propose_client_migration` | `contracts/escrow/src/migration.rs` | L36–L76 |
| `accept_client_migration` | `contracts/escrow/src/migration.rs` | L78–L109 |
| `submit_work_evidence` | `contracts/escrow/src/milestones.rs` | L65–L120 |
| TTL constants | `contracts/escrow/src/ttl.rs` | L45–L61 |
| TTL extension helpers | `contracts/escrow/src/ttl.rs` | L134–L199 |
