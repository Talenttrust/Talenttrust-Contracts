# Authorization and Access Control Rules

This document describes the access control, authorization rules, status transitions, and error handling for the TalentTrust Escrow contract.

## Roles

The contract defines four major roles:

1. **Operational Admin**
   - The address registered during `initialize(admin)`.
   - Controls contract-wide administrative parameters (pausing, emergency control, fee configurations, and admin transfer).

2. **Client**
   - The party funding the contract (the buyer/employer).
   - Responsible for creating contracts, depositing funds, approving milestone releases (under `ClientOnly` or `ClientAndArbiter` modes), initiating refunds, raising disputes, canceling contracts, and issuing reputation feedback.

3. **Freelancer**
   - The party receiving funds (the worker).
   - Responsible for submitting work evidence, approving milestone releases (under `MultiSig` mode), raising disputes, and executing releases once approved.

4. **Arbiter**
   - An independent third-party mediator (optional).
   - Responsible for approving milestone releases (under `ArbiterOnly` or `ClientAndArbiter` modes), resolving disputes, and finalizing contracts.

## Entrypoint Authorization Matrix

The table below outlines who can call each entrypoint, the required authorization, the contract states allowed, and the error rejections.

| Entrypoint | Authorized Caller | State / Preconditions | Rejections / Error Codes |
|---|---|---|---|
| `initialize(admin)` | Anyone (first call only) | Contract must not be initialized | `AlreadyInitialized` |
| `bind_settlement_token(admin, token)` | Admin (`admin.require_auth()`) | Initialized | `SettlementTokenAlreadyBound`, `InvalidSettlementToken`, `SettlementTokenIsSelf`, `SettlementTokenIsAdmin` |
| `pause()` | Admin (`admin.require_auth()`) | Initialized | `NotInitialized` |
| `unpause()` | Admin (`admin.require_auth()`) | Initialized | `NotInitialized` |
| `activate_emergency_pause()` | Admin (`admin.require_auth()`) | Anyone (if uninitialized), Admin (if initialized) | `NotInitialized` |
| `resolve_emergency()` | Admin (`admin.require_auth()`) | Initialized | `NotInitialized` |
| `set_protocol_fee_bps(new_bps)` | Admin (`admin.require_auth()`) | Initialized | `NotInitialized`, `new_bps` must be <= 10,000 |
| `withdraw_protocol_fees(amount, to)` | Admin (`admin.require_auth()`) | Initialized | `NotInitialized`, `InsufficientAccumulatedFees` |
| `propose_governance_admin(proposed)` | Admin (`admin.require_auth()`) | Initialized | `NotInitialized` |
| `accept_governance_admin()` | Proposed Admin (`pending_admin.require_auth()`) | Initialized, proposal is active | `TimelockNotElapsed` |
| `cancel_governance_admin_proposal()` | Admin (`admin.require_auth()`) | Initialized | `NotInitialized` |
| `create_contract(...)` | Client (`client.require_auth()`) | Initialized | `NotInitialized`, `EmptyMilestones`, `TooManyMilestones`, `InvalidParticipants`, `MissingArbiter`, `InvalidArbiter` |
| `deposit_funds(contract_id, caller, amount)` | Client (`caller.require_auth()`, must match contract client) | Status: `Created` or `PartiallyFunded` | `ContractNotFound`, `UnauthorizedRole`, `ContractPaused`, `EmergencyActive`, `ContractCancelled`, `ContractRefunded`, `InvalidDepositAmount`, `AmountMustBePositive` |
| `approve_milestone_release(contract_id, caller, index)` | `Client` / `Freelancer` / `Arbiter` (`caller.require_auth()`, depends on authorization mode) | Status: `Funded` or `PartiallyFunded` | `ContractNotFound`, `InvalidState`, `IndexOutOfBounds`, `MilestoneAlreadyReleased`, `UnauthorizedRole`, `AlreadyApproved` |
| `release_milestone(contract_id, caller, index)` | `Client` / `Freelancer` / `Arbiter` (`caller.require_auth()`, depends on authorization mode) | Status: `Funded` | `ContractNotFound`, `InvalidState`, `UnauthorizedRole`, `InsufficientApprovals`, `AlreadyReleased` |
| `refund_unreleased_milestones(contract_id, indices)` | Client (`contract.client.require_auth()`) | Status: `Created`, `Funded`, or `Disputed` | `ContractNotFound`, `EmptyRefundRequest`, `DuplicateMilestoneInRefund`, `IndexOutOfBounds`, `AlreadyReleased`, `AlreadyRefunded`, `InsufficientFunds` |
| `cancel_contract(contract_id, client)` | Client (`client.require_auth()`) | Status: `Created` or `Funded`, `released_amount` must be 0 | `ContractNotFound`, `UnauthorizedRole`, `AlreadyCancelled`, `InvalidStatusTransition`, `AlreadyFinalized` |
| `submit_work_evidence(contract_id, caller, index, evidence)` | Freelancer (`caller.require_auth()`) | Status: `Funded` | `ContractNotFound`, `UnauthorizedRole`, `InvalidState`, `EvidenceTooLong`, `MilestoneAlreadyReleased` |
| `issue_reputation(contract_id, caller, rating, comment)` | Client (`caller.require_auth()`) | Status: `Completed` | `ContractNotFound`, `UnauthorizedRole`, `InvalidRating`, `EmptyComment`, `CommentTooLong`, `SelfRating`, `ReputationAlreadyIssued` |
| `raise_dispute(contract_id, caller)` | Client or Freelancer (`caller.require_auth()`) | Status: `Funded` or `PartiallyFunded` | `ContractNotFound`, `UnauthorizedRole`, `ArbiterRequired`, `InvalidState` |
| `resolve_dispute(contract_id, arbiter, resolution)` | Arbiter (`arbiter.require_auth()`) | Status: `Disputed` | `ContractNotFound`, `UnauthorizedRole`, `InvalidStatusTransition`, `InvalidDisputeSplit` |
| `finalize_contract(contract_id, finalizer)` | Client, Freelancer, or Arbiter (`finalizer.require_auth()`) | Status: `Completed` or `Disputed` | `ContractNotFound`, `UnauthorizedRole`, `AlreadyFinalized`, `InvalidStatusTransition` |

## Status Transitions

The contract moves between statuses based on funding, completions, disputes, and cancellations.

```
       [Created] ───────────────────────────> [Cancelled]
           │                                      ▲
           │ (deposit_funds)                      │ (cancel_contract)
           ▼                                      │
  [PartiallyFunded] <─── (deposit_funds) ────> [Funded]
           │                                      │
           │ (raise_dispute)                      │ (release_milestone /
           ▼                                      │  refund_unreleased_milestones)
       [Disputed] <───── (raise_dispute) ─────────┘
           │
           │ (resolve_dispute /
           │  refund_unreleased_milestones)
           ▼
    [Completed] / [Refunded]
```

- **`Created`**
  - Initial state after contract creation.
  - Allowed transitions:
    - To `PartiallyFunded`: Client deposits an amount less than the total milestones.
    - To `Funded`: Client deposits the full total milestones.
    - To `Cancelled`: Client cancels the contract before any milestone funding/releases.

- **`PartiallyFunded`**
  - Contract has been partially funded.
  - Allowed transitions:
    - To `Funded`: Client deposits the remaining balance.
    - To `Disputed`: Client or Freelancer raises a dispute.

- **`Funded`**
  - Contract is fully funded and active.
  - Allowed transitions:
    - To `Completed`: All milestones are released or refunded (with at least one release).
    - To `Refunded`: All milestones are refunded.
    - To `Disputed`: Client or Freelancer raises a dispute.
    - To `Cancelled`: Client cancels the contract (only if `released_amount` is 0).

- **`Disputed`**
  - A dispute has been raised. Milestone releases are blocked.
  - Allowed transitions:
    - To `Completed`: Arbiter resolves the dispute with a full payout, partial refund/payout, or split resolution (or if some milestones were released before the dispute).
    - To `Refunded`: Arbiter resolves the dispute with a 100% full refund of all funded amounts.

- **`Completed`**
  - Terminal state where work is finished and all funds are distributed (released or refunded). Reputation can now be issued.

- **`Cancelled` / `Refunded`**
  - Terminal states where the contract is aborted and all remaining funded balances are returned to the client.

## Worked Example

Below is a step-by-step example of a contract lifecycle using the `ClientAndArbiter` release authorization mode.

### 1. Contract Creation
- **Action**: Client calls `create_contract` specifying freelancer and arbiter addresses, a milestone list, and `ReleaseAuthorization::ClientAndArbiter` mode.
- **Authorization**: Client signs the transaction.
- **State change**: A new contract is registered with status `Created`.

### 2. Deposit Funds
- **Action**: Client calls `deposit_funds` with the total milestone amount.
- **Authorization**: Client signs the transaction. The contract transfers the settlement tokens from the client to the contract's custody.
- **State change**: Status transitions from `Created` to `Funded`.

### 3. Submission of Work
- **Action**: Freelancer calls `submit_work_evidence` for Milestone 0, attaching work evidence.
- **Authorization**: Freelancer signs the transaction.
- **State change**: Milestone 0 evidence is updated. Status remains `Funded`.

### 4. Approval of Milestone
- **Action**: Client calls `approve_milestone_release` for Milestone 0.
- **Authorization**: Client signs the transaction.
- **State change**: Milestone 0 approvals record `client_approved = true`. Status remains `Funded`.

### 5. Release Milestone
- **Action**: Freelancer calls `release_milestone` for Milestone 0.
- **Authorization**: Freelancer signs the transaction. The contract verifies that client approval is present, transfers the milestone amount (minus protocol fee) to the freelancer, and accumulates the protocol fee.
- **State change**: Milestone 0 transitions to `released = true`. Since other milestones are unreleased, status remains `Funded`.

### 6. Dispute Resolution
- **Action**: Client raises a dispute via `raise_dispute` because of disagreement over Milestone 1.
- **Authorization**: Client signs the transaction.
- **State change**: Status transitions from `Funded` to `Disputed`.
- **Action**: Arbiter resolves the dispute via `resolve_dispute` with a `Split` resolution (50% refund to client, 50% payout to freelancer).
- **Authorization**: Arbiter signs the transaction. The contract transfers the split payouts.
- **State change**: Status transitions from `Disputed` to `Completed`. The contract grants a pending reputation credit to the freelancer.

### 7. Reputation Feedback
- **Action**: Client calls `issue_reputation` to rate the freelancer (e.g., rating = 4, comment = "Good communication").
- **Authorization**: Client signs the transaction.
- **State change**: Consumes the pending reputation credit and updates the freelancer's reputation record.

### 8. Finalization
- **Action**: Freelancer calls `finalize_contract`.
- **Authorization**: Freelancer signs the transaction.
- **State change**: Writes the finalization record. The contract is now locked against any future mutations.
