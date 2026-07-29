# Settlement Model

This document outlines the settlement data model, the core accounting invariants, and the entrypoints that mutate custody balances within the Talenttrust Escrow contracts. Understanding these mechanics is essential for auditors and integrators interacting with the escrow lifecycle.

## Settlement Data Model

The escrow contract maintains an internal accounting ledger that tracks the lifecycle of funds for a specific contract. This accounting is entirely on-chain and mirrors the actual token balances held in the Stellar Asset Contract (SAC).

### Core Accounting Fields
The `Contract` struct tracks three primary cumulative fields:
- **`funded_amount`**: The total amount of tokens (in stroops) that the client has successfully deposited into the escrow via SAC transfers.
- **`released_amount`**: The total amount of tokens (in stroops) that have been released (paid out) to the freelancer. This includes both the freelancer's net payout and the accumulated protocol fees retained by the contract.
- **`refunded_amount`**: The total amount of tokens (in stroops) that have been refunded back to the client.

### Milestone Tracking
Each `Milestone` struct tracks its own state, which maps to the contract's cumulative fields:
- `amount`: The target funding for this milestone.
- `released`: A boolean flag indicating if the milestone has been paid out.
- `refunded`: A boolean flag indicating if the milestone has been refunded.

## Core Invariants

The integrity of the escrow system relies on strict accounting invariants. These are enforced before any state mutation or token transfer occurs.

### 1. The Refundable Balance Invariant
The `refundable_balance` represents the amount of tokens currently locked in escrow that can still be released or refunded.
```text
refundable_balance = funded_amount - released_amount - refunded_amount
```

**Guarantees:**
- **Non-negative**: `refundable_balance >= 0` at all times. The contract can never become insolvent.
- **Additive Decomposition**: At any point in the lifecycle, `funded_amount == released_amount + refunded_amount + refundable_balance`.
- **Terminal Zero**: `refundable_balance` reaches `0` strictly when all milestones are either `released` or `refunded`.

### 2. Deposit Cap Invariant
A contract can never hold more funds than the sum of its milestones.
```text
funded_amount <= SUM(milestone.amount)
```
Over-funding is prevented during the deposit preflight check via `checked_add`, panicking with `InvalidDepositAmount` if this limit is breached.

### 3. Atomic SAC Custody
The contract's accounting fields are never updated unless the underlying SAC `transfer` succeeds. 
- During a deposit, the `token::Client::transfer(client, escrow, amount)` is executed before `funded_amount` is increased.
- During a release or refund, the outward transfer is executed before `released_amount` or `refunded_amount` is increased.
If a SAC transfer fails (e.g., insufficient balance or frozen trustline), the transaction reverts, leaving the accounting state untouched.

## Entrypoints Mutating Settlement State

Only three entrypoints are authorized to mutate the settlement state. All three are guarded by the emergency circuit breaker (`ContractPaused` / `EmergencyActive`).

### `deposit_funds`
- **Action**: Pulls tokens from the client to the escrow contract.
- **State Change**: Increases `funded_amount` by the deposited amount.
- **Status Update**: Transitions the contract to `PartiallyFunded` or `Funded` (if `funded_amount == SUM(milestone.amount)`).

### `release_milestone`
- **Action**: Pushes tokens from the escrow contract to the freelancer (net of the protocol fee) and retains the fee.
- **State Change**: Increases `released_amount` by the milestone's full `amount`. Sets the milestone's `released` flag to `true`.
- **Status Update**: Transitions the contract to `Completed` if all milestones are released.

### `refund_unreleased_milestones`
- **Action**: Pushes unreleased tokens from the escrow contract back to the client.
- **State Change**: Increases `refunded_amount` by the sum of the refunded milestones. Sets the `refunded` flag to `true` on those milestones.
- **Status Update**: Transitions the contract to `Refunded` if the entire `funded_amount` has been refunded.

## Worked Example

Let's trace a 2-milestone contract through a partial release and a refund.

**1. Creation**
- Milestone 1: 100 USDC
- Milestone 2: 150 USDC
- Total Required: 250 USDC
- **State**: `funded_amount = 0`, `released_amount = 0`, `refunded_amount = 0`. `refundable_balance = 0`.

**2. Full Deposit**
- The client deposits 250 USDC. 
- SAC transfers 250 USDC from Client to Escrow.
- **State**: `funded_amount = 250`, `released_amount = 0`, `refunded_amount = 0`. `refundable_balance = 250`.

**3. Release Milestone 1**
- The client approves and releases Milestone 1 (100 USDC).
- Assuming a 5% protocol fee (5 USDC).
- SAC transfers 95 USDC from Escrow to Freelancer. (Escrow retains 5 USDC for protocol fees).
- **State**: `funded_amount = 250`, `released_amount = 100`, `refunded_amount = 0`. `refundable_balance = 150`.

**4. Refund Milestone 2**
- A dispute occurs, or the client/freelancer agree to cancel the remaining work. Milestone 2 (150 USDC) is refunded.
- SAC transfers 150 USDC from Escrow to Client.
- **State**: `funded_amount = 250`, `released_amount = 100`, `refunded_amount = 150`. `refundable_balance = 0`.

At the end of this flow, `funded_amount (250) == released_amount (100) + refunded_amount (150) + refundable_balance (0)`. The invariant holds perfectly.
