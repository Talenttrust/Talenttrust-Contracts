# Events Storage Layout and TTL Policy

## Current Status

The escrow contract (`contracts/escrow/src/lib.rs`) is currently in a skeleton implementation phase. Events are not yet emitted - all functions return placeholder values without any event emissions. The comments in the code indicate:

> "Full implementation would store state in persistent storage."

This document describes the **intended** event layout and TTL/bump strategy based on the contract structure and Soroban best practices.

## Intended Event Layout

### Event Types

The following events are planned for the escrow contract based on its public functions:

#### ContractCreated Event

**Emitted by**: `create_contract` (line 28-37 in `contracts/escrow/src/lib.rs`)

**Topics**:
- `Symbol::from_short("contract_created")` - Event type identifier
- `Address` - Client address
- `Address` - Freelancer address
- `u32` - Contract ID

**Data**:
- `Vec<i128>` - Milestone amounts

**Purpose**: Notifies listeners when a new escrow contract is created with its participants and payment structure.

#### FundsDeposited Event

**Emitted by**: `deposit_funds` (line 39-43 in `contracts/escrow/src/lib.rs`)

**Topics**:
- `Symbol::from_short("funds_deposited")` - Event type identifier
- `u32` - Contract ID
- `Address` - Client address

**Data**:
- `i128` - Deposit amount (in stroops)

**Purpose**: Notifies listeners when funds are deposited into an escrow contract.

#### MilestoneReleased Event

**Emitted by**: `release_milestone` (line 45-49 in `contracts/escrow/src/lib.rs`)

**Topics**:
- `Symbol::from_short("milestone_released")` - Event type identifier
- `u32` - Contract ID
- `u32` - Milestone ID
- `Address` - Freelancer address

**Data**:
- `i128` - Released amount (in stroops)

**Purpose**: Notifies listeners when a milestone payment is released to the freelancer.

#### ReputationIssued Event

**Emitted by**: `issue_reputation` (line 51-55 in `contracts/escrow/src/lib.rs`)

**Topics**:
- `Symbol::from_short("reputation_issued")` - Event type identifier
- `Address` - Freelancer address
- `u32` - Contract ID

**Data**:
- `i128` - Rating value

**Purpose**: Notifies listeners when a reputation credential is issued to a freelancer after contract completion.

#### ContractStatusChanged Event

**Emitted by**: Various functions when contract status changes

**Topics**:
- `Symbol::from_short("status_changed")` - Event type identifier
- `u32` - Contract ID
- `ContractStatus` - New status (Created, Funded, Completed, Disputed)

**Data**: None

**Purpose**: Notifies listeners when the contract status transitions between states.

## Event Implementation in Soroban

### Event Emission Pattern

Events in Soroban are emitted using the `env.events()` API:

```rust
// Example implementation for ContractCreated event
env.events()
    .publish(
        (
            symbol_short!("contract_created"),
            client.clone(),
            freelancer.clone(),
            contract_id,
        ),
        milestone_amounts,
    );
```

### Event Storage Characteristics

Unlike persistent storage, events in Soroban have different characteristics:

1. **Immutability**: Once emitted, events cannot be modified or deleted
2. **Ledger History**: Events are stored in the ledger history and can be queried
3. **No TTL**: Events do not have a TTL in the same sense as persistent storage entries
4. **Queryability**: Events can be queried by event type, topics, and contract address

## TTL/Bump Strategy for Events

### Event TTL Overview

Events in Soroban do not require explicit TTL management like persistent storage because:

- Events are part of the immutable ledger history
- They are retained according to the network's archival policy
- No bump operations are needed for events

### Related TTL Considerations

While events themselves don't need TTL management, the **contract instance** that emits events does require TTL bumping:

- **Contract Instance TTL**: Must be bumped on every function call that emits events
- **Implementation**: Use `env.storage().instance().extend_ttl()` before event emission

### Example Event Emission with TTL Bump

```rust
pub fn create_contract(env: Env, client: Address, freelancer: Address, milestone_amounts: Vec<i128>) -> u32 {
    // Bump contract instance TTL before emitting event
    env.storage().instance().extend_ttl(100, 518_400);
    
    // Emit ContractCreated event
    env.events()
        .publish(
            (
                symbol_short!("contract_created"),
                client.clone(),
                freelancer.clone(),
                contract_id,
            ),
            milestone_amounts.clone(),
        );
    
    // Store contract data in persistent storage
    // ... storage operations ...
    
    contract_id
}
```

## Cross-Reference to Code

### Contract Creation

**Function**: `create_contract` (line 28-37 in `contracts/escrow/src/lib.rs`)

**Intended Event**: `ContractCreated`

**Current Status**: Returns placeholder value, no event emission

### Fund Deposit

**Function**: `deposit_funds` (line 39-43 in `contracts/escrow/src/lib.rs`)

**Intended Event**: `FundsDeposited`

**Current Status**: Returns `true`, no event emission

### Milestone Release

**Function**: `release_milestone` (line 45-49 in `contracts/escrow/src/lib.rs`)

**Intended Event**: `MilestoneReleased`

**Current Status**: Returns `true`, no event emission

### Reputation Issuance

**Function**: `issue_reputation` (line 51-55 in `contracts/escrow/src/lib.rs`)

**Intended Event**: `ReputationIssued`

**Current Status**: Returns `true`, no event emission

## Event Querying

### Query Patterns

Clients can query events using:

1. **By Contract**: All events emitted by a specific contract
2. **By Event Type**: All events of a specific type (e.g., all `contract_created` events)
3. **By Topics**: Events matching specific topic values (e.g., events for a specific contract ID)
4. **Time Range**: Events within a specific ledger range

### Example Query

```rust
// Query all milestone release events for a specific contract
let events = env.events()
    .filter(|event| {
        event.topics[0] == symbol_short!("milestone_released") 
            && event.topics[1] == contract_id
    })
    .collect();
```

## Implementation Notes

1. **Event Ordering**: Events are emitted in the order they occur within a transaction
2. **Gas Costs**: Event emission consumes gas; consider event frequency in gas optimization
3. **Indexing**: Design topics to enable efficient querying by common access patterns
4. **Data Size**: Keep event data payloads minimal to reduce gas costs
5. **Privacy**: Events are public on the ledger; avoid sensitive data in event payloads

## References

- Soroban SDK Documentation: https://docs.soroban.stellar.org/
- Soroban Events: https://docs.soroban.stellar.org/docs/learn/events
- Contract Code: `contracts/escrow/src/lib.rs`
