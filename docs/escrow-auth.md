# Escrow Authorization and Access Control Rules

This document specifies the authorization, access control rules, role privileges, allowed state transitions, and failure/rejection conditions for the TalentTrust Escrow smart contract (`contracts/escrow`).

---

## 1. Overview

The TalentTrust escrow contract manages milestone-based payments, client migrations, dispute resolution, reputation scoring, and protocol governance on Soroban (Stellar). To protect user funds and maintain system safety, every state-modifying entrypoint enforces strict authorization guards using:

1. **Soroban `require_auth()` Authentication**: Ensures transactions are cryptographically signed by the required participant address before any state mutation occurs.
2. **Role-Based Authorization**: Restricts function execution to specific roles (Governance Admin, Client, Freelancer, Arbiter, Proposed Admin, or Proposed Client).
3. **State Machine Guardrails**: Enforces valid `ContractStatus` transitions (e.g. `Created` → `Funded` → `Completed`) and rejects invalid state mutations.
4. **Emergency & Pause Controls**: Provides global system freeze capabilities (`Paused`, `EmergencyActive`) that halt all money movement and state modifications.
5. **Contract Finalization**: Locks completed/disputed contracts against any further mutations (`AlreadyFinalized`).

---

## 2. Roles & Privilege Matrix

The contract defines six distinct roles plus an unauthenticated public tier.

| Role | Identification / Storage Key | Capabilities & Authority | Primary Entrypoints |
| --- | --- | --- | --- |
| **Governance Admin (`Admin`)** | Stored in `DataKey::Admin` via `initialize` | Full protocol governance authority. Controls protocol fee rates, governed parameters, emergency/pause controls, admin rotation proposals, and protocol fee withdrawals. | `initialize`, `bind_settlement_token`, `set_protocol_fee_bps`, `set_governed_params`, `propose_governance_admin`, `cancel_governance_admin_proposal`, `pause`, `unpause`, `activate_emergency_pause`, `resolve_emergency`, `withdraw_protocol_fees` |
| **Proposed Admin (`PendingAdmin`)** | Stored in `DataKey::PendingAdmin` | Nominated address for admin rotation. Can accept the admin role after the minimum timelock delay has elapsed. | `accept_governance_admin` |
| **Client (`client`)** | Stored per escrow contract in `Contract.client` | Escrow buyer/funder. Creates contracts, deposits funds, approves milestone releases (mode-dependent), proposes/cancels client migrations, requests milestone refunds, cancels unfunded/unreleased contracts, opens disputes, issues reputation feedback, and finalizes contracts. | `create_contract`, `deposit_funds`, `approve_milestone_release`, `refund_unreleased_milestones`, `cancel_contract`, `propose_client_migration`, `cancel_client_migration`, `raise_dispute`, `issue_reputation`, `finalize_contract` |
| **Proposed Client (`new_client`)** | Stored in temporary `DataKey::PendingClientMigration` | Nominated address for client migration. Can accept migration to replace the current client. | `accept_client_migration` |
| **Freelancer (`freelancer`)** | Stored per escrow contract in `Contract.freelancer` | Escrow service provider. Submits work evidence, approves milestone releases (MultiSig mode), triggers releases (MultiSig mode), opens disputes, receives milestone payouts and reputation credits, and finalizes contracts. | `approve_milestone_release`, `release_milestone`, `submit_work_evidence`, `raise_dispute`, `finalize_contract` |
| **Arbiter (`arbiter`)** | Optional per-contract in `Contract.arbiter` | Independent dispute resolver. Approves milestone releases (`ArbiterOnly`, `ClientAndArbiter` modes), triggers releases (`ArbiterOnly`, `ClientAndArbiter` modes), resolves open disputes, and finalizes contracts. | `approve_milestone_release`, `release_milestone`, `resolve_dispute`, `finalize_contract` |
| **Public / Unauthenticated** | Any caller address | Read-only inspection of contract state, bounds, readiness checklist, milestones, approvals, finalization records, and reputation statistics. Performs no state mutation and requires no signature. | `get_contract`, `get_contract_summary`, `get_bounds`, `get_mainnet_readiness_info`, `is_paused`, `is_emergency`, `contract_exists`, `get_milestones`, `get_milestone`, `get_milestone_approvals`, etc. |

---

## 3. Allowed State Transitions

### Contract Lifecycle States (`ContractStatus`)

```
               ┌──────────────┐
               │   Created    │
               └──────┬───────┘
                      │
           deposit_funds (full)
                      │
                      ▼
               ┌──────────────┐
       ┌───────┤    Funded    ├──────┐
       │       └──────┬───────┘      │
       │              │              │
cancel_contract   raise_dispute   release_milestone / refund_unreleased_milestones
       │              │              │
       ▼              ▼              ▼
┌──────────────┐┌────────────┐┌──────────────┐
│  Cancelled   ││  Disputed  ││  Completed   │ (All milestones released or partially refunded)
└──────────────┘└─────┬──────┘└──────────────┘
                      │
               resolve_dispute
                      │
                      ▼
        ┌───────────────────────────┐
        │  Refunded / Completed /   │
        │      PartiallyFunded      │
        └───────────────────────────┘
```

| Current Status | Allowed Action / Entrypoint | Target Status | Required Role | Conditions & Notes |
| --- | --- | --- | --- | --- |
| *(None)* | `create_contract` | `Created` | Client | Initializes contract record with zero balance and status `Created`. |
| `Created` | `deposit_funds` | `Funded` | Client | Advances to `Funded` when total deposited equals aggregate milestone amount. |
| `Created` | `cancel_contract` | `Cancelled` | Client | Refunds any partial deposit; terminal state. |
| `Created` | `refund_unreleased_milestones` | `Refunded` | Client | Refunds unreleased overdue/no-deadline milestones. Transitions to `Refunded` if all milestones refunded. |
| `Created` | `propose_client_migration` | `Created` | Client | Stages pending client migration proposal. |
| `Funded` | `approve_milestone_release` | `Funded` | Mode Approver | Records milestone approval in temporary storage. |
| `Funded` | `release_milestone` | `Funded` / `Completed` | Mode Releaser | Deducts fee, pays freelancer. Transitions to `Completed` when all milestones are released/refunded. |
| `Funded` | `submit_work_evidence` | `Funded` | Freelancer | Records deliverable hash/URL (max 256 bytes). |
| `Funded` | `refund_unreleased_milestones` | `Funded` / `Refunded` / `Completed` | Client | Refunds unreleased overdue/no-deadline milestones. Transitions to `Refunded` if all refunded, or `Completed` if some released and remainder refunded. |
| `Funded` | `cancel_contract` | `Cancelled` | Client | Allowed only if `released_amount == 0`. Full balance returned to client. |
| `Funded` | `propose_client_migration` | `Funded` | Client | Stages pending client migration proposal. |
| `Funded` | `raise_dispute` | `Disputed` | Client / Freelancer | Freezes milestone releases; requires assigned arbiter. |
| `PartiallyFunded` | `approve_milestone_release` | `PartiallyFunded` | Mode Approver | Stages milestone approval. |
| `PartiallyFunded` | `raise_dispute` | `Disputed` | Client / Freelancer | Transitions contract to `Disputed`. |
| `Disputed` | `resolve_dispute` | `Refunded` / `Completed` / `Funded` | Arbiter | Applies `FullRefund`, `PartialRefund`, `FullPayout`, or `Split`. |
| `Disputed` | `refund_unreleased_milestones` | `Refunded` / `Completed` | Client | Client can refund unreleased overdue milestones during dispute. |
| `Completed` | `issue_reputation` | `Completed` | Client | Rate freelancer (1-5) + comment (1-200 bytes). Flags `reputation_issued = true`. |
| `Completed` | `finalize_contract` | `Completed` | Client / Freelancer / Arbiter | Writes immutable finalization snapshot. Prevents further mutations. |
| `Disputed` | `finalize_contract` | `Disputed` | Client / Freelancer / Arbiter | Writes immutable finalization snapshot. Prevents further mutations. |
| `Cancelled` | *(None)* | *(Terminal)* | None | Immutable terminal state. Rejects all mutating entrypoints. |
| `Refunded` | *(None)* | *(Terminal)* | None | Immutable terminal state. Rejects all mutating entrypoints. |

---

## 4. Entrypoint Authorization Specification

### 4.1 Initialization & Settlement Binding

| Entrypoint | Authorized Role | Signature / Target Auth | Prerequisites | Rejections & Error Codes |
| --- | --- | --- | --- | --- |
| `initialize(env, admin)` | Admin | `admin.require_auth()` | Single-use only | `AlreadyInitialized` if called more than once. |
| `bind_settlement_token(env, admin, token)` | Admin | `admin.require_auth()` | `require_initialized`, `admin == stored_admin` | `NotInitialized` if contract uninitialized; `UnauthorizedRole` if caller != stored admin; `SettlementTokenAlreadyBound` if token already set; `SettlementTokenIsSelf` if `token == self`; `SettlementTokenIsAdmin` if `token == admin`; `InvalidSettlementToken` if SAC balance probe panics. |

### 4.2 Governance & Protocol Parameters

| Entrypoint | Authorized Role | Signature / Target Auth | Prerequisites | Rejections & Error Codes |
| --- | --- | --- | --- | --- |
| `set_protocol_fee_bps(env, new_bps)` | Admin | `admin.require_auth()` | `require_initialized` | `NotInitialized` if uninitialized; `UnauthorizedRole` if caller != admin; panics if `new_bps > 10_000`. |
| `set_governed_params(env, admin, fee_bps, max_total)` | Admin | `admin.require_auth()` | `require_initialized`, `admin == stored_admin` | `NotInitialized`; `UnauthorizedRole`; `InvalidProtocolParameters` if `fee_bps > 10_000`. |

### 4.3 Admin Rotation (Two-Step Transfer)

Admin rotation uses a mandatory two-step proposal and timelock pattern to prevent accidental lockout.

| Entrypoint | Authorized Role | Signature / Target Auth | Prerequisites | Rejections & Error Codes |
| --- | --- | --- | --- | --- |
| `propose_governance_admin(env, proposed)` | Current Admin | `admin.require_auth()` | `require_initialized` | `NotInitialized`; `UnauthorizedRole`. |
| `accept_governance_admin(env)` | Proposed Admin | `pending_admin.require_auth()` | `require_initialized`, `PendingAdmin` exists, `elapsed_ledgers >= 17_280` | `NotInitialized`; `InvalidState` if no proposal exists; `TimelockNotElapsed` if delay < 17,280 ledgers (~24 hours). |
| `cancel_governance_admin_proposal(env)` | Current Admin | `admin.require_auth()` | `require_initialized`, `PendingAdmin` exists | `NotInitialized`; `UnauthorizedRole`; `InvalidState` if no proposal active. |

### 4.4 Pause & Emergency Controls

Global controls apply across all contracts managed by the escrow instance.

| Entrypoint | Authorized Role | Signature / Target Auth | Prerequisites | Rejections & Error Codes |
| --- | --- | --- | --- | --- |
| `pause(env)` | Admin | `admin.require_auth()` | `require_initialized` | `NotInitialized`; `UnauthorizedRole`. |
| `unpause(env)` | Admin | `admin.require_auth()` | `require_initialized`, `Emergency == false` | `NotInitialized`; `UnauthorizedRole`; `EmergencyActive` if emergency pause is active. |
| `activate_emergency_pause(env)` | Admin | `admin.require_auth()` (if initialized) | None | Sets both `Emergency` and `Paused` flags. |
| `resolve_emergency(env)` | Admin | `admin.require_auth()` | `require_initialized` | `NotInitialized`; `UnauthorizedRole`. Clears both `Emergency` and `Paused` flags. |

### 4.5 Escrow Contract Creation & Funding

| Entrypoint | Authorized Role | Signature / Target Auth | Prerequisites | Rejections & Error Codes |
| --- | --- | --- | --- | --- |
| `create_contract(env, client, freelancer, arbiter, milestones, release_auth)` | Client | `client.require_auth()` | `require_not_paused` | `ContractPaused` / `EmergencyActive`; `InvalidParticipant` if `client == freelancer`; `MissingArbiter` if mode requires arbiter and `arbiter.is_none()`; `InvalidArbiter` if `arbiter == client` or `arbiter == freelancer`; `EmptyMilestones` if `milestones.is_empty()`; `TooManyMilestones` if count > 10; `InvalidMilestoneAmount` if any amount <= 0; `TotalCapExceeded` if total > governed cap. |
| `deposit_funds(env, contract_id, caller, amount)` | Client | `caller.require_auth()` | `require_initialized`, `require_not_paused`, `caller == contract.client` | `NotInitialized`; `ContractPaused`; `SettlementTokenNotConfigured`; `ContractNotFound`; `UnauthorizedRole` if `caller != client`; `InvalidState` if status != `Created`; `AmountMustBePositive` if `amount <= 0`. |

### 4.6 Milestone Approvals & Release

Milestone releases are governed by four `ReleaseAuthorization` modes.

#### Release Authorization Mode Matrix

| Mode | Enum | Allowed Approvers | Required Approval Condition | Allowed Release Callers |
| --- | --- | --- | --- | --- |
| `ClientOnly` | `0` | Client | `client_approved == true` | Client |
| `ClientAndArbiter` | `1` | Client OR Arbiter | `client_approved || arbiter_approved` | Client OR Arbiter |
| `ArbiterOnly` | `2` | Arbiter | `arbiter_approved == true` | Arbiter |
| `MultiSig` | `3` | Client AND Freelancer | `client_approved && freelancer_approved` | Client OR Freelancer |

| Entrypoint | Authorized Role | Signature / Target Auth | Prerequisites | Rejections & Error Codes |
| --- | --- | --- | --- | --- |
| `approve_milestone_release(env, contract_id, caller, milestone_index)` | Participant (Mode dependent) | `caller.require_auth()` | `require_not_paused`, `require_not_finalized`, status in `[Funded, PartiallyFunded]` | `ContractPaused`; `AlreadyFinalized`; `ContractNotFound`; `InvalidState`; `IndexOutOfBounds`; `MilestoneAlreadyReleased`; `UnauthorizedRole` (if caller role invalid for mode); `AlreadyApproved` (if same party approves twice). |
| `release_milestone(env, contract_id, caller, milestone_index)` | Participant (Mode dependent) | `caller.require_auth()` | `require_not_paused`, `require_not_finalized`, status == `Funded`, required approvals present | `ContractPaused`; `AlreadyFinalized`; `ContractNotFound`; `InvalidState`; `UnauthorizedRole`; `IndexOutOfBounds`; `MilestoneAlreadyReleased`; `AlreadyRefunded`; `InsufficientApprovals` / `ApprovalExpired`; `InsufficientFunds`. |

### 4.7 Refunds & Contract Cancellation

| Entrypoint | Authorized Role | Signature / Target Auth | Prerequisites | Rejections & Error Codes |
| --- | --- | --- | --- | --- |
| `refund_unreleased_milestones(env, contract_id, indices)` | Client | `contract.client.require_auth()` | `require_not_paused`, `require_not_finalized`, status in `[Created, Funded, Disputed]` | `ContractPaused`; `AlreadyFinalized`; `ContractNotFound`; `EmptyRefundRequest`; `DuplicateMilestoneInRefund`; `InvalidState`; `IndexOutOfBounds`; `AlreadyReleased`; `AlreadyRefunded`; `MilestoneNotOverdue` (if deadline set and `now <= deadline`); `InsufficientFunds`. |
| `cancel_contract(env, contract_id, client)` | Client | `client.require_auth()` | `require_not_paused`, `require_not_finalized`, `client == contract.client` | `ContractPaused`; `AlreadyFinalized`; `ContractNotFound`; `UnauthorizedRole`; `AlreadyCancelled`; `InvalidStatusTransition` (if status not `Created`/`Funded` or `released_amount > 0`). |

### 4.8 Client Migration Lifecycle

Client migration transfers client rights and responsibilities to a new address.

| Entrypoint | Authorized Role | Signature / Target Auth | Prerequisites | Rejections & Error Codes |
| --- | --- | --- | --- | --- |
| `propose_client_migration(env, contract_id, current_client, new_client)` | Current Client | `current_client.require_auth()` | `require_not_paused`, `require_not_finalized`, `current_client == contract.client` | `ContractPaused`; `AlreadyFinalized`; `ContractNotFound`; `UnauthorizedRole`; `InvalidParticipant` (if `new_client` is current client or freelancer); `InvalidStatusTransition` (if status in `[Completed, Cancelled, Refunded, Disputed]`); `InvalidState` (if pending migration already active). |
| `accept_client_migration(env, contract_id, new_client)` | Proposed Client | `new_client.require_auth()` | `require_not_paused`, `require_not_finalized`, pending proposal exists | `ContractPaused`; `AlreadyFinalized`; `ContractNotFound`; `InvalidStatusTransition`; `InvalidState` (no pending proposal); `UnauthorizedRole` (if `new_client` != proposed address). |
| `cancel_client_migration(env, contract_id, current_client)` | Current Client | `current_client.require_auth()` | `require_not_paused`, `require_not_finalized`, `current_client == contract.client` | `ContractPaused`; `AlreadyFinalized`; `ContractNotFound`; `UnauthorizedRole`; `InvalidState` (no pending proposal). |

### 4.9 Dispute Management & Resolution

| Entrypoint | Authorized Role | Signature / Target Auth | Prerequisites | Rejections & Error Codes |
| --- | --- | --- | --- | --- |
| `raise_dispute(env, contract_id, caller)` | Client OR Freelancer | `caller.require_auth()` | `require_initialized`, `require_not_paused`, `require_not_finalized`, status in `[Funded, PartiallyFunded]` | `NotInitialized`; `ContractPaused`; `AlreadyFinalized`; `ContractNotFound`; `UnauthorizedRole` (caller not client/freelancer); `ArbiterRequired` (no arbiter assigned); `InvalidState`. |
| `resolve_dispute(env, contract_id, arbiter, resolution)` | Assigned Arbiter | `arbiter.require_auth()` | `require_initialized`, `require_not_paused`, `require_not_finalized`, status == `Disputed`, `arbiter == contract.arbiter` | `NotInitialized`; `ContractPaused`; `AlreadyFinalized`; `ContractNotFound`; `InvalidStatusTransition` (status != `Disputed`); `UnauthorizedRole` (caller != assigned arbiter); `InvalidDisputeSplit` (split sum != remaining balance). |

### 4.10 Work Evidence & Reputation Feedback

| Entrypoint | Authorized Role | Signature / Target Auth | Prerequisites | Rejections & Error Codes |
| --- | --- | --- | --- | --- |
| `submit_work_evidence(env, contract_id, caller, index, evidence)` | Freelancer | `caller.require_auth()` | `require_initialized`, `require_not_paused`, `require_not_finalized`, `caller == contract.freelancer`, status == `Funded` | `NotInitialized`; `ContractPaused`; `AlreadyFinalized`; `ContractNotFound`; `UnauthorizedRole`; `InvalidState`; `IndexOutOfBounds`; `MilestoneAlreadyReleased`; `AlreadyRefunded`; `EvidenceTooLong` (length > 256 bytes). |
| `issue_reputation(env, contract_id, caller, rating, comment)` | Client | `caller.require_auth()` | `require_not_paused`, `caller == contract.client`, status == `Completed`, `reputation_issued == false` | `ContractPaused`; `ContractNotFound`; `UnauthorizedRole`; `InvalidRating` (not 1-5); `EmptyComment` (0 bytes); `CommentTooLong` (> 200 bytes); `NotCompleted`; `ReputationAlreadyIssued`; `SelfRating` (`client == freelancer`); `InvalidState` (no pending credits). |

### 4.11 Contract Finalization & Protocol Fee Withdrawal

| Entrypoint | Authorized Role | Signature / Target Auth | Prerequisites | Rejections & Error Codes |
| --- | --- | --- | --- | --- |
| `finalize_contract(env, contract_id, finalizer)` | Client, Freelancer, OR Arbiter | `finalizer.require_auth()` | `require_not_paused`, status in `[Completed, Disputed]`, no finalization record exists | `ContractPaused`; `ContractNotFound`; `AlreadyFinalized`; `UnauthorizedRole` (finalizer not participant); `InvalidStatusTransition` (status not `Completed` or `Disputed`). |
| `withdraw_protocol_fees(env, amount, to)` | Admin | `admin.require_auth()` | `require_initialized`, `require_not_paused`, `amount <= accumulated_fees` | `NotInitialized`; `ContractPaused`; `UnauthorizedRole`; `AmountMustBePositive` (`amount <= 0`); `InsufficientAccumulatedFees` (`amount > accumulated`). |

### 4.12 Read-Only Inspection Entrypoints (Unauthenticated)

The following functions perform no state mutations, enforce no caller authorization checks, and are publicly queryable by anyone:

- `get_admin(env)`
- `get_governance_admin(env)`
- `get_protocol_fee_bps(env)`
- `get_governed_parameters(env)`
- `get_pending_admin_proposed_at(env)`
- `get_bounds(env)`
- `get_mainnet_readiness_info(env)`
- `get_settlement_token(env)`
- `is_settlement_token_bound(env)`
- `is_paused(env)`
- `is_emergency(env)`
- `get_contract(env, contract_id)`
- `contract_exists(env, contract_id)`
- `get_next_contract_id(env)`
- `get_contract_summary(env, contract_id)`
- `get_milestones(env, contract_id)`
- `get_milestone(env, contract_id, milestone_index)`
- `get_refundable_balance(env, contract_id)`
- `get_milestone_approvals(env, contract_id, milestone_index)`
- `get_approval_deadline(env, contract_id, milestone_index)`
- `get_finalization_record(env, contract_id)`
- `has_pending_client_migration(env, contract_id)`
- `get_pending_client_migration(env, contract_id)`
- `is_milestone_overdue(env, contract_id, milestone_index)`
- `get_accumulated_protocol_fees(env)`
- `get_reputation(env, address)`
- `get_average_rating(env, address)`
- `get_pending_reputation_credits(env, address)`
- `get_reputation_comment(env, contract_id)`
- `get_work_evidence(env, contract_id, milestone_index)`

---

## 5. Rejection Rules & Error Catalog

Every error code returned by the escrow contract represents a specific authorization, validation, or security guard.

| Error Enum Variant | Numeric Code | Description & Trigger Cause |
| --- | --- | --- |
| `InvalidParticipant` | `1` | Client and freelancer are identical addresses, or proposed client is freelancer/client. |
| `EmptyMilestones` | `2` | `create_contract` called with 0 milestones. |
| `InvalidMilestoneAmount` | `3` | Milestone amount is <= 0 stroops. |
| `InvalidDepositAmount` | `4` | Deposit amount exceeds remaining required funding or is invalid. |
| `InvalidMilestone` | `5` | Milestone index is out of range. |
| `ContractNotFound` | `6` | Specified `contract_id` does not exist in persistent storage. |
| `EmptyRefundRequest` | `7` | `refund_unreleased_milestones` called with an empty index list. |
| `DuplicateMilestoneInRefund` | `8` | The same milestone index appears twice in a refund request vector. |
| `AlreadyReleased` | `9` | Milestone has already been released to the freelancer. |
| `AlreadyRefunded` | `10` | Milestone has already been refunded to the client. |
| `InsufficientFunds` | `11` | Contract balance is insufficient for requested payout/refund/release. |
| `AlreadyInitialized` | `12` | `initialize` called when contract is already initialized. |
| `InsufficientAccumulatedFees` | `13` | `withdraw_protocol_fees` requested an amount exceeding accrued fees. |
| `NotInitialized` | `14` | Entrypoint required initialization but `initialize` has not been called. |
| `UnauthorizedRole` | `15` | Caller signature does not match the required role for the operation. |
| `ContractPaused` | `16` | Mutating operation attempted while contract is paused. |
| `EmergencyActive` | `17` | Mutating operation or `unpause` attempted while emergency pause is active. |
| `InvalidState` | `18` | Contract status is incompatible with the requested operation. |
| `InvalidRating` | `19` | Reputation rating is outside `[1, 5]`. |
| `SelfRating` | `20` | Client attempted to issue reputation feedback to themselves. |
| `ReputationAlreadyIssued` | `21` | Reputation feedback has already been submitted for this contract. |
| `NotCompleted` | `22` | `issue_reputation` called on a contract that is not in `Completed` status. |
| `FreelancerMismatch` | `23` | Target freelancer address does not match contract's stored freelancer. |
| `InvalidStatusTransition` | `24` | Requested status change violates the contract state machine rules. |
| `ArbiterRequired` | `25` | `raise_dispute` called on a contract with no assigned arbiter. |
| `InvalidDisputeSplit` | `26` | Custom dispute resolution split sum does not match remaining balance. |
| `AccountingInvariantViolated` | `27` | Balance conservation invariant (`released + refunded + fees <= funded`) failed. |
| `PotentialOverflow` | `28` | Checked arithmetic detected potential integer overflow. |
| `AlreadyFinalized` | `29` | Mutating operation attempted on a finalized contract. |
| `AmountMustBePositive` | `30` | Deposit or fee withdrawal amount is <= 0. |
| `SettlementTokenNotConfigured` | `31` | Money movement attempted before `bind_settlement_token` was called. |
| `SettlementTokenAlreadyBound` | `32` | `bind_settlement_token` called when settlement token is already bound. |
| `TotalCapExceeded` | `33` | Total milestone sum exceeds the governed maximum escrow total. |
| `TooManyMilestones` | `34` | Number of milestones exceeds `MAX_MILESTONES` (10). |
| `MissingArbiter` | `35` | Arbiter is required by release authorization mode but was not provided. |
| `InvalidArbiter` | `36` | Arbiter address is identical to client or freelancer address. |
| `ContractCancelled` | `37` | Value-moving operation attempted on a cancelled contract. |
| `ContractRefunded` | `38` | Value-moving operation attempted on a fully refunded contract. |
| `InvalidSettlementToken` | `39` | Settlement token address failed SAC balance probe. |
| `SettlementTokenIsSelf` | `40` | Attempted to bind the escrow contract's own address as settlement token. |
| `SettlementTokenIsAdmin` | `41` | Attempted to bind the admin address as settlement token. |
| `EmptyComment` | `42` | Reputation feedback comment is 0 bytes. |
| `CommentTooLong` | `43` | Reputation feedback comment exceeds 200 bytes. |

---

## 6. Worked Example: Complete Escrow Lifecycle

Below is an accurate, end-to-end worked example tracing authorization checks, roles, state changes, and rejections across a full contract lifecycle.

### Setup & Governance Configuration
- **Admin**: `GADMIN...`
- **Token Contract**: `GTOKEN...` (Stellar Asset Contract)
- **Protocol Fee**: 250 basis points (2.5%)

```rust
// 1. Admin initializes the contract
Escrow::initialize(env, GADMIN); // Requires GADMIN.require_auth()

// 2. Admin binds the SAC settlement token
Escrow::bind_settlement_token(env, GADMIN, GTOKEN); // Requires GADMIN.require_auth()

// 3. Admin sets protocol fee to 2.5% (250 bps)
Escrow::set_protocol_fee_bps(env, 250); // Requires GADMIN.require_auth()
```

### Contract Creation & Funding
- **Client**: `GCLIENT...`
- **Freelancer**: `GFREELANCER...`
- **Arbiter**: `GARBITER...`
- **Milestones**: Milestone 0 = 600 USDC (600,000,000 stroops), Milestone 1 = 400 USDC (400,000,000 stroops)
- **Release Authorization**: `ClientAndArbiter` (Mode 1)

```rust
// 4. Client creates contract #1
let contract_id = Escrow::create_contract(
    env,
    GCLIENT,
    GFREELANCER,
    Some(GARBITER),
    vec![600_000_000, 400_000_000],
    ReleaseAuthorization::ClientAndArbiter
);
// - Auth check: GCLIENT.require_auth() succeeds.
// - Validation: GCLIENT != GFREELANCER; GARBITER is distinct; milestones non-empty.
// - State created: Contract ID 1, Status = Created, total_deposited = 0.

// Rejection test (Unauthorized deposit):
// If GFREELANCER attempts to deposit funds:
Escrow::deposit_funds(env, 1, GFREELANCER, 1_000_000_000); 
// -> Panics with EscrowError::UnauthorizedRole (caller != contract.client)

// 5. Client deposits full 1,000 USDC
Escrow::deposit_funds(env, 1, GCLIENT, 1_000_000_000);
// - Auth check: GCLIENT.require_auth() succeeds.
// - SAC Transfer: Transfers 1,000_000_000 stroops from GCLIENT to Escrow contract.
// - State transition: Created -> Funded. funded_amount = 1_000_000_000.
```

### Milestone 0: Work Evidence, Approval & Release

```rust
// 6. Freelancer submits work evidence for Milestone 0
Escrow::submit_work_evidence(env, 1, GFREELANCER, 0, String::from_str(&env, "ipfs://Qm123..."));
// - Auth check: GFREELANCER.require_auth() succeeds.
// - State updated: Milestone 0 work_evidence set.

// 7. Client approves Milestone 0 release
Escrow::approve_milestone_release(env, 1, GCLIENT, 0);
// - Auth check: GCLIENT.require_auth() succeeds.
// - State created: Temporary MilestoneApprovals(1, 0) created with client_approved = true.

// 8. Client releases Milestone 0 (600 USDC)
Escrow::release_milestone(env, 1, GCLIENT, 0);
// - Auth check: GCLIENT.require_auth() succeeds.
// - Mode check: ClientAndArbiter allows GCLIENT; client_approved is true.
// - Fee calculation: Gross = 600,000,000. Fee (2.5%) = 15,000,000 stroops. Net = 585,000,000 stroops.
// - SAC Transfer: Transfers 585,000,000 stroops from Escrow to GFREELANCER.
// - Fee accounting: AccumulatedProtocolFees += 15,000,000.
// - Approvals cleared: Temporary approval record deleted.
// - State updated: released_amount = 585,000,000 stroops. Milestone 0 released = true.
```

### Milestone 1: Dispute & Arbiter Resolution

```rust
// 9. Freelancer raises dispute on Milestone 1
Escrow::raise_dispute(env, 1, GFREELANCER);
// - Auth check: GFREELANCER.require_auth() succeeds.
// - Arbiter check: GARBITER is present.
// - State transition: Funded -> Disputed.

// Rejection test (Blocked release during dispute):
// If GCLIENT attempts to approve or release while Disputed:
Escrow::approve_milestone_release(env, 1, GCLIENT, 1);
// -> Panics with EscrowError::InvalidState (status != Funded)

// 10. Arbiter resolves dispute with a 50/50 split of remaining 400 USDC (200 USDC each)
Escrow::resolve_dispute(
    env,
    1,
    GARBITER,
    DisputeResolution::Split(DisputeSplit { client_amount: 200_000_000, freelancer_amount: 200_000_000 })
);
// - Auth check: GARBITER.require_auth() succeeds (GARBITER == contract.arbiter).
// - Balance check: client_amount (200m) + freelancer_amount (200m) == remaining (400m).
// - Accounting updated: refunded_amount += 200_000_000; released_amount += 200_000_000.
// - Status transition: Disputed -> Completed (since all funds are accounted for and freelancer received payout).
// - Reputation credit: PendingReputationCredits(GFREELANCER) += 1.
```

### Post-Completion: Reputation & Finalization

```rust
// 11. Client issues reputation rating (5 stars + comment)
Escrow::issue_reputation(env, 1, GCLIENT, 5, String::from_str(&env, "Great work on milestone 0!"));
// - Auth check: GCLIENT.require_auth() succeeds.
// - Preconditions: Status == Completed; reputation_issued == false; pending credits > 0.
// - State updated: Contract reputation_issued = true; GFREELANCER reputation updated; pending credit decremented.

// 12. Freelancer finalizes the contract record
Escrow::finalize_contract(env, 1, GFREELANCER);
// - Auth check: GFREELANCER.require_auth() succeeds (GFREELANCER is contract participant).
// - State written: Immutable FinalizationRecord saved under DataKey::Finalization(1).

// Rejection test (Mutation after finalization):
// If any party attempts to modify contract #1 now:
Escrow::raise_dispute(env, 1, GCLIENT);
// -> Panics with EscrowError::AlreadyFinalized
```

### Protocol Fee Withdrawal

```rust
// 13. Admin withdraws accrued 15 USDC protocol fees to treasury
Escrow::withdraw_protocol_fees(env, 15_000_000, GTREASURY);
// - Auth check: GADMIN.require_auth() succeeds.
// - Balance check: 15,000,000 <= AccumulatedProtocolFees (15,000,000).
// - SAC Transfer: Transfers 15,000,000 stroops from Escrow to GTREASURY.
// - State updated: AccumulatedProtocolFees = 0.
```
