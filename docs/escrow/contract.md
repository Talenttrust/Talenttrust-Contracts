# Escrow Contract Documentation

## Overview
The Escrow Contract is a Rust smart contract built for the Soroban platform. It provides a secure way for clients and freelancers to handle payments with milestones, ensuring that funds are released only when work is verified.

This contract includes:

- Contract creation between a client and freelancer
- Milestone-based payments
- Secure fund deposit and release
- Reputation issuance for freelancers
- Automated unit tests to verify correctness

## Contract Structure
### Types
ContractStatus: Represents the state of an escrow contract. Values:
- Created – Contract created but not funded
- Funded – Client has deposited funds
- Completed – All milestones completed
- Disputed – Issue flagged for dispute
- Cancelled – Contract cancelled by authorized party
- Refunded – Unreleased milestones refunded

Milestone: Defines a payment milestone:

amount: i128 – payment amount  
released: bool – whether the milestone has been paid

EscrowContract: Holds the full contract data:

client: Address – client address  
freelancer: Address – freelancer address  
arbiter: Option<Address> – optional arbiter for dispute resolution  
milestones: Vec<i128> – milestone payment amounts  
status: ContractStatus – current state  
total_deposited: i128 – total amount deposited  
released_amount: i128 – total amount released to freelancer

## Functions
### create_contract(env, client, freelancer, arbiter, milestone_amounts) -> u32
- Creates a new escrow contract.
- Stores the client, freelancer, and optional arbiter addresses.
- Sets up milestones with specified amounts.
- Validates arbiter doesn't overlap with client or freelancer.
- Returns a contract_id.
- Initial status: Created

### deposit_funds(env, contract_id, token, client, amount) -> bool
- Deposits funds into escrow.
- Only the client can call this.
- Updates contract status to Funded after success.
- Returns true if successful.

### release_milestone(env, contract_id, token, freelancer, amount) -> bool
- Releases a milestone payment to the freelancer.
- Only the freelancer can receive payments.
- Updates contract status to Completed after success.
- Returns true if successful.

### cancel_contract(env, contract_id, caller) -> bool
- Cancels an escrow contract under strict authorization and state constraints.
- Emits deterministic lifecycle event payload for indexer consumption.

**Authorization Rules:**
- Created state: Client or Freelancer can cancel
- Funded state: 
  - Client (only if zero milestones released)
  - Freelancer (economic deterrent - funds return to client)
  - Arbiter (dispute resolution)
- Disputed state: Arbiter only

**State Transitions:**
- Created → Cancelled ✓
- Funded → Cancelled ✓ (with conditions)
- Disputed → Cancelled ✓ (arbiter only)
- Completed → Cancelled ✗ (blocked - terminal state)
- Cancelled → Cancelled ✗ (idempotent error)

**Event Emission:**
Emits lifecycle event with:
- Topics: `("escrow", "v1", "cancel", contract_id)`
- Data: `(status: ContractStatus, amount: i128, milestone_index: u32, actor: Option<Address>, timestamp: u64)`

**Security Guarantees:**
- Cryptographic authorization required (caller.require_auth())
- Prevents retroactive cancellation of completed contracts
- Prevents double-cancellation (idempotency guard)
- Protects freelancer: client cannot cancel after milestone releases
- Arbiter isolation: cannot overlap with client/freelancer

### issue_reputation(env, freelancer, rating) -> bool
- Issues a reputation score for the freelancer after contract completion.
- Returns true.

### hello(env, to) -> Symbol
- Simple test function to verify contract interaction.
- Returns the same symbol passed in.

## Security Considerations
- Only the client can deposit funds.
- Only the freelancer can receive milestone payments.
- Milestone amounts must be greater than zero.
- Handles non-existent contracts safely using Option.
- Skips token transfers during unit tests to prevent errors.
- Always validate addresses before calling contract functions.
- Arbiter cannot be the same as client or freelancer.
- Cancellation requires cryptographic authorization from eligible parties.
- Completed contracts cannot be cancelled (prevents retroactive actions).
- Double-cancellation is prevented with explicit error.

## Contract Lifecycle

```
Created ──────────────→ Funded ───────────→ Completed
   │                      │                     │
   │                      │                     ✗ (no cancellation)
   ↓                      ↓
Cancelled ←───────────────┘
   │
   ↓ (Disputed)
Disputed ──────────────→ Cancelled (arbiter only)
```

**Key Transitions:**
- Created → Funded: Client deposits funds
- Created → Cancelled: Client or freelancer cancels
- Funded → Cancelled: Client (no releases), freelancer, or arbiter cancels
- Funded → Completed: All milestones released
- Funded → Disputed: Dispute raised
- Disputed → Cancelled: Arbiter cancels
- Completed: Terminal state (no further transitions)

## Testing
All core functions are covered with unit tests.
Tests include:
- Contract creation
- Fund deposit
- Milestone release
- Invalid deposit handling
- Hello-world function check

## Deterministic Event Schema

The escrow lifecycle uses a shared event schema for deterministic indexing:

- Topic tuple: `("escrow", "v1", operation, contract_id)`
- Data tuple: `(status, amount, milestone_index, actor, timestamp)`

Lifecycle operations covered:

- `create_contract` -> operation `create`
- `deposit_funds` -> operation `deposit`
- `approve_milestone` -> operation `approve`
- `release_milestone` -> operation `release`
- `cancel_contract` -> operation `cancel`

Breaking change note:

- Consumers listening to legacy cancellation topic `contract_cancelled` must migrate to the v1 lifecycle event topic.
