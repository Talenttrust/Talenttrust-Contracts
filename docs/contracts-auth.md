# Contract Authorization and Access Control Rules

**Document Version:** 1.0  
**Contract Version:** Soroban Escrow Contract  
**Last Updated:** 2026-07-27

## Table of Contents

1. [Overview](#overview)
2. [Roles and Participants](#roles-and-participants)
3. [Authorization Patterns](#authorization-patterns)
4. [Contract States](#contract-states)
5. [Entrypoint Authorization Matrix](#entrypoint-authorization-matrix)
6. [Release Authorization Modes](#release-authorization-modes)
7. [State Transition Rules](#state-transition-rules)
8. [Error Codes](#error-codes)
9. [Security Properties](#security-properties)
10. [Worked Examples](#worked-examples)

---

## Overview

This document provides a comprehensive reference for the authorization and access control rules enforced by the TalentTrust escrow smart contract. It describes:

- **Who** can call each entrypoint
- **When** (in which contract states) operations are allowed
- **What** preconditions must be met
- **How** the contract rejects unauthorized attempts

All authorization checks are implemented in `contracts/escrow/src/authorization.rs` and enforced across entrypoints in `contracts/escrow/src/lib.rs` and submodules.

---

## Roles and Participants

The escrow contract recognizes four distinct roles:

###
 1. Admin

**Definition:** The governance address that controls protocol-level operations.

**Authority:**
- Initialize the contract
- Pause/unpause contract operations
- Activate/deactivate emergency mode
- Configure protocol parameters (fees, limits, settlement token)
- Rotate admin via two-step proposal/acceptance
- Set arbiters for contracts
- Configure dispute parameters

**Storage Key:** `DataKey::Admin`  
**Set During:** `initialize(admin: Address)`  
**Authentication:** `admin.require_auth()` enforced by `load_and_auth_admin()` helper

### 2. Client

**Definition:** The party requesting work and funding the escrow.

**Authority:**
- Create contracts
- Deposit funds into contracts
- Approve milestone releases (mode-dependent)
- Trigger milestone releases (mode-dependent)
- Request refunds for unreleased milestones
- Cancel unfunded contracts
- Raise disputes
- Issue reputation feedback
- Propose client migration

**Per-Contract:** Stored in `Contract.client`  
**Authentication:** `client.require_auth()` at each relevant entrypoint

### 3. Freelancer

**Definition:** The party providing services and receiving milestone payments.

**Authority:**
- Accept contracts (if acceptance flow is implemented)
- Approve milestone releases (in MultiSig mode only)
- Trigger milestone releases (in MultiSig mode only)
- Cancel unfunded contracts (with client agreement)
- Raise disputes
- Submit work evidence for milestones

**Per-Contract:** Stored in `Contract.freelancer`  
**Authentication:** `freelancer.require_auth()` at each relevant entrypoint

### 4. Arbiter

**Definition:** An optional third-party designated to resolve disputes.

**Authority:**
- Approve milestone releases (in ArbiterOnly or ClientAndArbiter modes)
- Trigger milestone releases (in ArbiterOnly or ClientAndArbiter modes)
- Resolve disputes with binding decisions

**Per-Contract:** Stored in `Contract.arbiter: Option<Address>`  
**Required For:** `ReleaseAuthorization::ArbiterOnly` and `ReleaseAuthorization::ClientAndArbiter` modes  
**Authentication:** `arbiter.require_auth()` at each relevant entrypoint

---

## Authorization Patterns

The contract uses three primary authorization patterns:

### Pattern 1: Single-Role Authorization

**Used For:** Admin operations, client-only operations

**Implementation:**
```rust
fn load_and_auth_admin(env: &Env) -> Address {
    let admin: Address = env
        .storage()
        .persistent()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| env.panic_with_error(Error::NotInitialized));
    admin.require_auth();
    admin
}
```

**Error:** `UnauthorizedRole` if caller is not the stored role holder

### Pattern 2: Multi-Role Authorization (OR logic)

**Used For:** Operations that can be performed by multiple roles

**Implementation:**
```rust
pub fn require_participant(env: &Env, caller: &Address, contract: &Contract) -> ParticipantRole {
    get_caller_role(caller, contract)
        .unwrap_or_else(|| env.panic_with_error(Error::UnauthorizedRole))
}
```

**Error:** `UnauthorizedRole` if caller is not any of the allowed roles

### Pattern 3: Release-Mode Authorization

**Used For:** Milestone approval and release operations

**Implementation:**
```rust
pub fn require_release_authorization(env: &Env, caller: &Address, contract: &Contract) {
    let role = get_caller_role(caller, contract);
    match contract.release_authorization {
        ReleaseAuthorization::ClientOnly => {
            if role != Some(ParticipantRole::Client) {
                env.panic_with_error(Error::UnauthorizedRole);
            }
        }
        // ... other modes
    }
}
```

**Error:** `UnauthorizedRole` if caller's role doesn't match the release mode

---

## Contract States

The escrow contract tracks per-contract state transitions:

| State | Enum Value | Description |
|-------|------------|-------------|
| `Created` | 0 | Contract created, awaiting initial funding |
| `Accepted` | 1 | Contract accepted by freelancer (if acceptance flow enabled) |
| `Funded` | 2 | Contract fully or partially funded, work in progress |
| `Completed` | 3 | All milestones released or refunded |
| `Disputed` | 4 | Contract under dispute, awaiting arbiter resolution |
| `Cancelled` | 5 | Contract cancelled before completion |
| `Refunded` | 6 | All funds refunded to client |
| `PartiallyFunded` | 7 | Some but not all milestone amounts deposited |

**Storage:** `Contract.status: ContractStatus`

---

## Entrypoint Authorization Matrix

### Initialization and Configuration

| Entrypoint | Allowed Roles | Required State | Preconditions | Errors |
|------------|---------------|----------------|---------------|--------|
| `initialize(admin)` | Any (first-time) | Not initialized | - Contract not already initialized | `AlreadyInitialized` |
| | | | - `admin.require_auth()` | |
| `bind_settlement_token(admin, token)` | Admin | Initialized, not paused | - Admin auth<br>- No token already bound<br>- Token is valid SAC<br>- Token ≠ self<br>- Token ≠ admin | `NotInitialized`<br>`UnauthorizedRole`<br>`SettlementTokenAlreadyBound`<br>`InvalidSettlementToken`<br>`SettlementTokenIsSelf`<br>`SettlementTokenIsAdmin` |
| `pause(admin)` | Admin | Initialized | - Admin auth | `NotInitialized`<br>`UnauthorizedRole` |
| `unpause(admin)` | Admin | Initialized | - Admin auth | `NotInitialized`<br>`UnauthorizedRole` |
| `activate_emergency_pause(admin)` | Admin | Initialized | - Admin auth | `NotInitialized`<br>`UnauthorizedRole` |
| `resolve_emergency(admin)` | Admin | Initialized | - Admin auth | `NotInitialized`<br>`UnauthorizedRole` |

### Contract Lifecycle

| Entrypoint | Allowed Roles | Required State | Preconditions | Errors |
|------------|---------------|----------------|---------------|--------|
| `create_contract(client, freelancer, arbiter, milestones, release_auth)` | Client | Initialized, not paused | - Client auth<br>- Valid participants<br>- Valid milestones<br>- Arbiter required for certain modes | `NotInitialized`<br>`ContractPaused`<br>`InvalidParticipant`<br>`EmptyMilestones`<br>`InvalidMilestoneAmount`<br>`TooManyMilestones`<br>`TotalCapExceeded`<br>`MissingArbiter`<br>`InvalidArbiter` |
| `deposit_funds(contract_id, from, amount)` | Client | Contract in `Created` or `PartiallyFunded` state | - Client auth<br>- Settlement token bound<br>- Valid deposit amount<br>- Not paused | `NotInitialized`<br>`ContractNotFound`<br>`UnauthorizedRole`<br>`InvalidDepositAmount`<br>`SettlementTokenNotConfigured` |
| `cancel_contract(contract_id, caller)` | Client or Freelancer | Contract in `Created` or `PartiallyFunded` (unfunded) | - Caller is client or freelancer<br>- Contract not yet funded<br>- Not finalized | `ContractNotFound`<br>`UnauthorizedRole`<br>`InvalidState`<br>`AlreadyFinalized` |

### Milestone Operations

| Entrypoint | Allowed Roles | Required State | Preconditions | Errors |
|------------|---------------|----------------|---------------|--------|
| `approve_milestone_release(contract_id, caller, milestone_idx)` | Client, Freelancer, or Arbiter (mode-dependent) | Contract in `Funded` state | - Caller auth<br>- Caller authorized per release mode<br>- Milestone not released<br>- Not duplicate approval | `ContractNotFound`<br>`InvalidState`<br>`UnauthorizedRole`<br>`IndexOutOfBounds`<br>`MilestoneAlreadyReleased`<br>`AlreadyApproved` |
| `release_milestone(contract_id, caller, milestone_idx)` | Client, Freelancer, or Arbiter (mode-dependent) | Contract in `Funded` state | - Caller auth<br>- Caller authorized per release mode<br>- Sufficient approvals<br>- Milestone not released<br>- Sufficient funds | `ContractNotFound`<br>`InvalidState`<br>`UnauthorizedRole`<br>`IndexOutOfBounds`<br>`MilestoneAlreadyReleased`<br>`InsufficientApprovals`<br>`InsufficientFunds` |
| `refund_unreleased_milestones(contract_id, caller, milestone_indices)` | Client or Arbiter | Contract in `Funded` state | - Caller is client or arbiter<br>- Milestones not released<br>- Sufficient refundable balance | `ContractNotFound`<br>`UnauthorizedRole`<br>`EmptyRefundRequest`<br>`DuplicateMilestoneInRefund`<br>`AlreadyReleased`<br>`InsufficientFunds` |
| `submit_work_evidence(contract_id, freelancer, milestone_idx, evidence)` | Freelancer | Any state | - Freelancer auth<br>- Valid evidence string<br>- Milestone exists | `ContractNotFound`<br>`FreelancerMismatch`<br>`IndexOutOfBounds`<br>`EvidenceTooLong` |

### Dispute Management

| Entrypoint | Allowed Roles | Required State | Preconditions | Errors |
|------------|---------------|----------------|---------------|--------|
| `raise_dispute(contract_id, caller, reason_hash)` | Client or Freelancer | Contract in `Funded` state | - Caller is client or freelancer<br>- No active dispute<br>- Arbiter assigned | `ContractNotFound`<br>`UnauthorizedRole`<br>`InvalidState`<br>`MissingArbiter` |
| `resolve_dispute(contract_id, arbiter, resolution)` | Arbiter | Contract in `Disputed` state | - Arbiter auth<br>- Valid resolution<br>- Sufficient funds for resolution | `ContractNotFound`<br>`UnauthorizedRole`<br>`InvalidState`<br>`InvalidDisputeSplit`<br>`InsufficientFunds` |

### Reputation and Feedback

| Entrypoint | Allowed Roles | Required State | Preconditions | Errors |
|------------|---------------|----------------|---------------|--------|
| `issue_reputation(contract_id, client, rating, comment)` | Client | Contract in `Completed` state | - Client auth<br>- Not already issued<br>- Valid rating (1-5)<br>- Valid comment | `ContractNotFound`<br>`UnauthorizedRole`<br>`NotCompleted`<br>`ReputationAlreadyIssued`<br>`InvalidRating`<br>`EmptyComment`<br>`CommentTooLong` |

### Admin Operations

| Entrypoint | Allowed Roles | Required State | Preconditions | Errors |
|------------|---------------|----------------|---------------|--------|
| `set_arbiter(contract_id, admin, new_arbiter)` | Admin | Any state | - Admin auth<br>- Valid arbiter (not client/freelancer)<br>- Arbiter required by release mode | `NotInitialized`<br>`UnauthorizedRole`<br>`ContractNotFound`<br>`InvalidArbiter`<br>`MissingArbiter` |
| `set_protocol_fee_bps(admin, fee_bps)` | Admin | Initialized | - Admin auth<br>- Valid fee (≤ MAX_BPS) | `NotInitialized`<br>`UnauthorizedRole` |
| `withdraw_protocol_fees(admin, amount)` | Admin | Initialized | - Admin auth<br>- Sufficient accumulated fees | `NotInitialized`<br>`UnauthorizedRole`<br>`InsufficientAccumulatedFees` |
| `propose_admin(admin, proposed)` | Admin | Initialized | - Admin auth | `NotInitialized`<br>`UnauthorizedRole` |
| `accept_admin(proposed)` | Proposed Admin | Proposal exists | - Proposed admin auth<br>- Timelock elapsed | `NotInitialized`<br>`UnauthorizedRole`<br>`TimelockNotElapsed` |

### Read-Only Operations (No Authorization Required)

| Entrypoint | Description |
|------------|-------------|
| `get_contract(contract_id)` | Returns full contract state |
| `get_contract_summary(contract_id)` | Returns contract summary with milestones |
| `get_milestones(contract_id)` | Returns all milestones for a contract |
| `get_milestone(contract_id, milestone_idx)` | Returns single milestone |
| `get_refundable_balance(contract_id)` | Returns available refund amount |
| `is_milestone_overdue(contract_id, milestone_idx)` | Checks if milestone deadline passed |
| `contract_exists(contract_id)` | Checks if contract ID is allocated |
| `get_next_contract_id()` | Returns next contract ID to be allocated |
| `get_admin()` | Returns stored admin address |
| `get_settlement_token()` | Returns bound settlement token |
| `is_settlement_token_bound()` | Checks if settlement token is bound |
| `get_bounds()` | Returns protocol-wide limits |
| `get_reputation(freelancer)` | Returns freelancer's reputation record |

---

## Release Authorization Modes

The contract supports four release authorization modes that determine who can approve and release milestones:

### Mode 1: ClientOnly

**Enum Value:** `ReleaseAuthorization::ClientOnly = 0`

**Approval Rules:**
- **Allowed Approvers:** Client only
- **Required Approvals:** 1 (client)
- **Approval Logic:** `approvals.client_approved == true`

**Release Rules:**
- **Allowed Release Callers:** Client only
- **Authorization Check:** `caller == contract.client`

**Use Case:** Client retains full control over milestone payments

**Contract Creation:** Arbiter optional

### Mode 2: ArbiterOnly

**Enum Value:** `ReleaseAuthorization::ArbiterOnly = 2`

**Approval Rules:**
- **Allowed Approvers:** Arbiter only
- **Required Approvals:** 1 (arbiter)
- **Approval Logic:** `approvals.arbiter_approved == true`

**Release Rules:**
- **Allowed Release Callers:** Arbiter only
- **Authorization Check:** `caller == contract.arbiter`

**Use Case:** All milestone releases require arbiter approval (escrow agent model)

**Contract Creation:** Arbiter **required** (`MissingArbiter` error if None)

### Mode 3: ClientAndArbiter

**Enum Value:** `ReleaseAuthorization::ClientAndArbiter = 1`

**Approval Rules:**
- **Allowed Approvers:** Client OR Arbiter
- **Required Approvals:** 1 (either client OR arbiter)
- **Approval Logic:** `approvals.client_approved || approvals.arbiter_approved`

**Release Rules:**
- **Allowed Release Callers:** Client OR Arbiter
- **Authorization Check:** `caller == contract.client || caller == contract.arbiter`

**Use Case:** Flexible control—either party can approve/release

**Contract Creation:** Arbiter **required** (`MissingArbiter` error if None)

### Mode 4: MultiSig

**Enum Value:** `ReleaseAuthorization::MultiSig = 3`

**Approval Rules:**
- **Allowed Approvers:** Client AND Freelancer
- **Required Approvals:** 2 (both client AND freelancer)
- **Approval Logic:** `approvals.client_approved && approvals.freelancer_approved`

**Release Rules:**
- **Allowed Release Callers:** Client OR Freelancer (after both approve)
- **Authorization Check:** `caller == contract.client || caller == contract.freelancer`

**Use Case:** Mutual agreement required before payment

**Contract Creation:** Arbiter optional

---

## State Transition Rules

### Valid State Transitions

```
Created → PartiallyFunded → Funded → Completed
   ↓           ↓              ↓         ↓
Cancelled  Cancelled      Disputed  (terminal)
                             ↓
                        Refunded / Completed
```

### Transition Triggers

| From State | To State | Triggered By | Authorization |
|------------|----------|--------------|---------------|
| `Created` | `PartiallyFunded` | `deposit_funds` (partial amount) | Client |
| `Created` | `Funded` | `deposit_funds` (full amount) | Client |
| `Created` | `Cancelled` | `cancel_contract` | Client or Freelancer |
| `PartiallyFunded` | `Funded` | `deposit_funds` (remaining amount) | Client |
| `PartiallyFunded` | `Cancelled` | `cancel_contract` | Client or Freelancer |
| `Funded` | `Completed` | Last milestone released/refunded | System (automatic) |
| `Funded` | `Disputed` | `raise_dispute` | Client or Freelancer |
| `Disputed` | `Completed` | `resolve_dispute` (full payout) | Arbiter |
| `Disputed` | `Refunded` | `resolve_dispute` (full refund) | Arbiter |
| `Disputed` | `Funded` | `resolve_dispute` (partial split) | Arbiter |

### Terminal States

| State | Description | Can Transition? |
|-------|-------------|-----------------|
| `Completed` | All milestones settled | **No** (terminal) |
| `Refunded` | All funds returned to client | **No** (terminal) |
| `Cancelled` | Contract cancelled before funding | **No** (terminal) |

---

## Error Codes

### Authorization Errors

| Error | Code | When Raised |
|-------|------|-------------|
| `UnauthorizedRole` | 11, 15 | Caller not authorized for the operation |
| `NotInitialized` | 14, 36 | Contract not initialized (admin not set) |
| `ContractPaused` | 16, 37 | Contract paused by admin |
| `EmergencyActive` | 17, 38 | Emergency mode active |

### Participant Validation Errors

| Error | Code | When Raised |
|-------|------|-------------|
| `InvalidParticipant` | 1, 31 | Participant address invalid or duplicated |
| `MissingArbiter` | 25, 42 | Arbiter required but not provided |
| `InvalidArbiter` | 13, 36 | Arbiter is same as client or freelancer |
| `FreelancerMismatch` | 21, 23 | Caller is not the contract's freelancer |

### State and Lifecycle Errors

| Error | Code | When Raised |
|-------|------|-------------|
| `ContractNotFound` | 6, 10 | Contract ID does not exist |
| `InvalidState` | 16, 18 | Operation not allowed in current contract state |
| `AlreadyFinalized` | 29, 46 | Contract finalized (immutable) |
| `AlreadyCancelled` | 50 | Contract already cancelled |
| `InvalidStatusTransition` | 24, 41 | State transition not allowed |

### Milestone and Approval Errors

| Error | Code | When Raised |
|-------|------|-------------|
| `IndexOutOfBounds` | 3 | Milestone index invalid |
| `AlreadyReleased` | 4, 9, 17 | Milestone already released |
| `AlreadyRefunded` | 8, 10 | Milestone already refunded |
| `MilestoneAlreadyReleased` | 17 | Duplicate release attempt |
| `AlreadyApproved` | 18 | Duplicate approval from same party |
| `InsufficientApprovals` | 18, 20 | Required approvals missing or expired |

### Financial Errors

| Error | Code | When Raised |
|-------|------|-------------|
| `InvalidMilestoneAmount` | 3, 26 | Milestone amount invalid (≤ 0 or > max) |
| `InvalidDepositAmount` | 4, 32 | Deposit amount invalid |
| `InsufficientFunds` | 9, 11 | Insufficient contract balance |
| `InsufficientAccumulatedFees` | 13, 35 | Not enough protocol fees to withdraw |
| `AmountMustBePositive` | 15, 30 | Amount ≤ 0 |
| `PotentialOverflow` | 28, 45 | Arithmetic overflow risk |
| `TotalCapExceeded` | 33 | Total milestone amount exceeds cap |

### Reputation Errors

| Error | Code | When Raised |
|-------|------|-------------|
| `InvalidRating` | 19, 22 | Rating not in range [1, 5] |
| `SelfRating` | 20, 39 | Client cannot rate themselves |
| `ReputationAlreadyIssued` | 21, 23 | Reputation feedback already given |
| `NotCompleted` | 22, 40 | Contract not in Completed state |
| `EmptyComment` | 29, 42 | Reputation comment empty |
| `CommentTooLong` | 30, 43 | Comment exceeds 200 bytes |

### Settlement and Configuration Errors

| Error | Code | When Raised |
|-------|------|-------------|
| `SettlementTokenNotConfigured` | 31, 52 | No settlement token bound |
| `SettlementTokenAlreadyBound` | 32 | Settlement token already set |
| `InvalidSettlementToken` | 39 | Token address not a valid SAC |
| `SettlementTokenIsSelf` | 40 | Cannot bind escrow contract as token |
| `SettlementTokenIsAdmin` | 41 | Cannot bind admin as token |

---

## Security Properties

### Fail-Closed Design

All authorization checks fail-closed:
- **Missing admin:** Panics with `NotInitialized`
- **Missing approvals:** Panics with `InsufficientApprovals`
- **Expired approvals:** Treated as missing (TTL eviction)
- **Unauthorized caller:** Panics with `UnauthorizedRole`
- **Invalid state:** Panics with `InvalidState`

### Authentication Guarantees

- All mutating operations require `require_auth()` from Soroban SDK
- Authentication enforced **before** any state mutation (Checks-Effects-Interactions)
- No privilege escalation possible (roles loaded from persistent storage)

### Approval Isolation

- Approvals stored per-milestone, not per-contract
- Approvals cleared after successful release
- TTL expiry prevents stale approvals (7-day default)
- Duplicate approvals rejected

### State Immutability

- Terminal states (`Completed`, `Refunded`, `Cancelled`) are immutable
- Finalized contracts reject all value-moving operations
- Emergency pause freezes all financial operations

---

## Worked Examples

### Example 1: ClientOnly Mode - Happy Path

**Scenario:** Client creates contract, deposits funds, approves and releases milestone

**Steps:**

1. **Create Contract**
   ```
   Caller: Client (authenticated)
   Function: create_contract(client, freelancer, None, [1000], ReleaseAuthorization::ClientOnly)
   Authorization: ✓ Client auth
   Result: Contract ID 1 created, status = Created
   ```

2. **Deposit Funds**
   ```
   Caller: Client (authenticated)
   Function: deposit_funds(1, client, 1000)
   Authorization: ✓ Client auth, client == contract.client
   Result: Contract status = Funded, funded_amount = 1000
   ```

3. **Approve Milestone**
   ```
   Caller: Client (authenticated)
   Function: approve_milestone_release(1, client, 0)
   Authorization: ✓ Client auth, ClientOnly mode allows client approval
   Result: MilestoneApprovals { client_approved: true, freelancer_approved: false, arbiter_approved: false }
   ```

4. **Release Milestone**
   ```
   Caller: Client (authenticated)
   Function: release_milestone(1, client, 0)
   Authorization: ✓ Client auth, ClientOnly mode allows client release
   Approval Check: ✓ client_approved = true
   Result: 1000 transferred to freelancer, milestone marked released, contract status = Completed
   ```

### Example 2: MultiSig Mode - Both Parties Must Approve

**Scenario:** Client and freelancer both approve before release

**Steps:**

1. **Create Contract**
   ```
   Caller: Client (authenticated)
   Function: create_contract(client, freelancer, None, [2000], ReleaseAuthorization::MultiSig)
   Authorization: ✓ Client auth
   Result: Contract ID 2 created, status = Created
   ```

2. **Deposit Funds**
   ```
   Caller: Client (authenticated)
   Function: deposit_funds(2, client, 2000)
   Authorization: ✓ Client auth, client == contract.client
   Result: Contract status = Funded
   ```

3. **Client Approves**
   ```
   Caller: Client (authenticated)
   Function: approve_milestone_release(2, client, 0)
   Authorization: ✓ Client auth, MultiSig mode allows client approval
   Result: MilestoneApprovals { client_approved: true, freelancer_approved: false, ... }
   ```

4. **Freelancer Tries to Release (Fails - Insufficient Approvals)**
   ```
   Caller: Freelancer (authenticated)
   Function: release_milestone(2, freelancer, 0)
   Authorization: ✓ Freelancer auth, MultiSig mode allows freelancer release
   Approval Check: ✗ client_approved && freelancer_approved = false
   Result: Panic with InsufficientApprovals
   ```

5. **Freelancer Approves**
   ```
   Caller: Freelancer (authenticated)
   Function: approve_milestone_release(2, freelancer, 0)
   Authorization: ✓ Freelancer auth, MultiSig mode allows freelancer approval
   Result: MilestoneApprovals { client_approved: true, freelancer_approved: true, ... }
   ```

6. **Freelancer Releases**
   ```
   Caller: Freelancer (authenticated)
   Function: release_milestone(2, freelancer, 0)
   Authorization: ✓ Freelancer auth, MultiSig mode allows freelancer release
   Approval Check: ✓ client_approved && freelancer_approved = true
   Result: 2000 transferred to freelancer, milestone released, contract status = Completed
   ```

### Example 3: Unauthorized Access Attempt

**Scenario:** External party attempts to release milestone

**Steps:**

1. **Contract Setup**
   ```
   Contract ID: 3
   Client: Alice
   Freelancer: Bob
   Mode: ClientOnly
   Status: Funded
   Milestone 0: Approved by Alice
   ```

2. **External Party Attempts Release**
   ```
   Caller: Charlie (authenticated, but not a participant)
   Function: release_milestone(3, charlie, 0)
   Authorization Check: get_caller_role(charlie, contract) = None
   Result: Panic with UnauthorizedRole (Charlie is not client, freelancer, or arbiter)
   ```

### Example 4: Dispute Flow with Arbiter Resolution

**Scenario:** Client raises dispute, arbiter resolves

**Steps:**

1. **Contract Setup**
   ```
   Contract ID: 4
   Client: Alice
   Freelancer: Bob
   Arbiter: Diana
   Mode: ClientAndArbiter
   Status: Funded
   ```

2. **Client Raises Dispute**
   ```
   Caller: Alice (client, authenticated)
   Function: raise_dispute(4, alice, reason_hash)
   Authorization: ✓ Alice is client (participant)
   Result: Contract status = Disputed, DisputeRecord created
   ```

3. **Freelancer Tries to Release (Fails - Invalid State)**
   ```
   Caller: Bob (freelancer, authenticated)
   Function: release_milestone(4, bob, 0)
   Authorization: ✓ Bob is freelancer
   State Check: Contract status = Disputed (not Funded)
   Result: Panic with InvalidState
   ```

4. **Arbiter Resolves Dispute**
   ```
   Caller: Diana (arbiter, authenticated)
   Function: resolve_dispute(4, diana, DisputeResolution::PartialRefund)
   Authorization: ✓ Diana is arbiter
   Result: Funds split 70% client / 30% freelancer, contract status = Completed
   ```

---

## Implementation References

**Authorization Module:**
- `contracts/escrow/src/authorization.rs` - Core authorization helpers
  - `get_caller_role()` - Determines caller's role
  - `require_release_authorization()` - Validates release authorization
  - `require_participant()` - Validates participant status
  - `require_admin()` - Validates admin auth

**Entrypoint Implementations:**
- `contracts/escrow/src/lib.rs` - Main contract entrypoints
- `contracts/escrow/src/release.rs` - Milestone release logic
- `contracts/escrow/src/refund.rs` - Refund logic
- `contracts/escrow/src/dispute.rs` - Dispute handling
- `contracts/escrow/src/governance.rs` - Admin operations

**Type Definitions:**
- `contracts/escrow/src/types.rs` - Enums for states, roles, errors

**Test Coverage:**
- `contracts/escrow/src/test/access_control.rs` - Authorization tests
- `contracts/escrow/src/test/security.rs` - Security-focused tests
- `contracts/escrow/src/authorization.rs` - Unit tests for auth helpers

---

## Related Documentation

- [`docs/escrow/authorization.md`](escrow/authorization.md) - Detailed release authorization modes
- [`docs/escrow/access-control.md`](escrow/access-control.md) - Access control implementation details
- [`docs/escrow/dispute-workflow.md`](escrow/dispute-workflow.md) - Dispute resolution flows
- [`docs/escrow/state-persistence.md`](escrow/state-persistence.md) - Contract state management

---

**Document Maintained By:** TalentTrust Development Team  
**Last Verification Against Source:** 2026-07-27  
**Contract Repository:** https://github.com/Talenttrust/Talenttrust-Contracts
