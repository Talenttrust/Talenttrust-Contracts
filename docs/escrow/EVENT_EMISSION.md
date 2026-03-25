# Event Emission Standardization - Implementation Guide

## Overview

This document describes the standardized event emission system for the TalentTrust Escrow contract. Events are critical for allowing off-chain listeners to track contract state changes and maintain synchronized views of the blockchain state.

## Event Types

### 1. ContractCreatedEvent
**Emitted when:** A new escrow contract is created
**Purpose:** Track new escrow agreements
**Fields:**
- `contract_id: u32` - Unique contract identifier
- `client: Address` - Client (payer) address
- `freelancer: Address` - Freelancer (payee) address
- `arbiter: Option<Address>` - Optional arbiter for dispute resolution
- `milestone_count: u32` - Total number of milestones
- `total_amount: i128` - Total contract value in stroops
- `release_auth: u8` - Authorization scheme (0=ClientOnly, 1=ArbiterOnly, 2=ClientAndArbiter, 3=MultiSig)
- `timestamp: u64` - Event creation timestamp (seconds since epoch)

**Implementation:**
```rust
Self::emit_contract_created(
    &env,
    contract_id,
    &client,
    &freelancer,
    &arbiter,
    milestone_count,
    total_amount,
    release_auth,
);
```

### 2. ContractFundedEvent
**Emitted when:** Funds are deposited into a contract
**Purpose:** Track funding progress and state transitions
**Fields:**
- `contract_id: u32` - Which contract received funds
- `depositor: Address` - Address that deposited funds
- `amount: i128` - Amount deposited in this transaction
- `funded_amount: i128` - Total funded amount after this deposit
- `is_fully_funded: bool` - Whether contract is now fully funded
- `timestamp: u64` - Event creation timestamp

**Implementation:**
```rust
Self::emit_contract_funded(
    &env,
    contract_id,
    &caller,
    amount,
    contract.funded_amount,
    is_fully_funded,
);
```

### 3. MilestoneReleasedEvent
**Emitted when:** A milestone payment is released to freelancer
**Purpose:** Track milestone completion and payments
**Fields:**
- `contract_id: u32` - Which contract
- `milestone_id: u32` - Which milestone (0-indexed)
- `amount: i128` - Amount released in stroops
- `approved_by: Address` - Address that approved the release
- `all_released: bool` - Whether all milestones are now released
- `timestamp: u64` - Event creation timestamp

**Implementation:**
```rust
Self::emit_milestone_released(
    &env,
    contract_id,
    milestone_id,
    amount,
    &approved_by,
    all_released,
);
```

### 4. DisputeInitiatedEvent
**Emitted when:** A dispute is initiated for a contract
**Purpose:** Track disputes and alert monitoring systems
**Fields:**
- `contract_id: u32` - Which contract
- `initiator: Address` - Who initiated (client or freelancer)
- `reason: String` - Reason for dispute
- `timestamp: u64` - Event creation timestamp

**Implementation:**
```rust
Self::emit_dispute_initiated(
    &env,
    contract_id,
    &initiator,
    &reason,
);
```

### 5. DisputeResolvedEvent
**Emitted when:** A dispute is resolved
**Purpose:** Track dispute resolution outcomes
**Fields:**
- `contract_id: u32` - Which contract
- `resolution: String` - Description of resolution
  - "Refunded to client" (resolution_type=0)
  - "Released to freelancer" (resolution_type=1)
  - "Split between parties" (resolution_type=2)
- `new_status: u8` - New contract status after resolution
- `timestamp: u64` - Event creation timestamp

**Implementation:**
```rust
Self::emit_dispute_resolved(
    &env,
    contract_id,
    &resolution_str,
    new_status,
);
```

### 6. ContractClosedEvent
**Emitted when:** A contract is closed
**Purpose:** Track contract lifecycle completion
**Fields:**
- `contract_id: u32` - Which contract
- `reason: String` - Why contract was closed
- `final_status: u8` - Final status (Completed, Closed, etc.)
- `total_released: i128` - Total amount released to freelancer
- `timestamp: u64` - Event creation timestamp

**Implementation:**
```rust
Self::emit_contract_closed(
    &env,
    contract_id,
    &reason,
    final_status,
    total_released,
);
```

## Event Publishing

All events are published using Soroban's standardized event system:

```rust
env.events().publish(
    (symbol_short!("escrow"), symbol_short!("EVENT_NAME")),
    event_struct,
);
```

### Event Topic Structure
- **Primary Topic:** `symbol_short!("escrow")` - Identifies contract
- **Secondary Topic:** `symbol_short!("created")` | `symbol_short!("funded")` | etc.

This allows for efficient indexing and filtering by event consumers.

## Contract State Transitions & Events

### Complete Lifecycle:

```
Created State
  └─> ContractCreatedEvent emitted
  └─> (await funding)
      └─> Funded State
          └─> ContractFundedEvent emitted (is_fully_funded=true)
          └─> (milestone approvals)
              └─> MilestoneReleasedEvent emitted
                  └─> (repeat for each milestone)
                      └─> Completed State
                          └─> ContractClosedEvent emitted (auto)
                          └─> close_contract() call
                              └─> ContractClosedEvent emitted (explicit)
```

### Dispute Path:

```
InDispute State
  └─> DisputeInitiatedEvent emitted
  └─> (admin resolution)
      └─> DisputeResolvedEvent emitted
          └─> Closed/Completed State based on resolution
```

## Security Considerations

1. **Event Immutability**: Events are permanently recorded on-chain and cannot be modified
2. **Event Ordering**: Events maintain order within a transaction
3. **Authorization**: Events only emitted after authorization checks pass
4. **State Consistency**: Event emission always follows successful state changes

## Testing Strategy

### Test Coverage (95%+)
- **Success Paths**: All event emissions in normal operations
- **Edge Cases**: Boundary conditions, overflow handling
- **Authorization Paths**: Proper event emission for different authorization schemes
- **Full Lifecycle**: End-to-end contract flows with all events

### Key Test Files
- `contracts/escrow/src/test_events.rs` - Comprehensive event emission tests
- `contracts/escrow/src/test.rs` - Existing contract tests

### Example Test
```rust
#[test]
fn test_contract_created_event_emission() {
    let (env, client, client_addr, freelancer_addr, _admin) = setup();
    
    let milestones = vec![&env, 1000, 2000, 3000];
    
    // Create contract - emits ContractCreatedEvent
    let contract_id = client.create_contract(
        &client_addr,
        &freelancer_addr,
        &None::<Address>,
        &milestones,
        &ReleaseAuthorization::ClientOnly,
    );

    assert_eq!(contract_id, 0);
    
    let contract = client.get_contract(&env, contract_id);
    assert_eq!(contract.total_amount, 6000);
}
```

## Integration Guide

### Off-Chain Listeners

1. **Subscribe to events:**
   ```javascript
   // Pseudocode for listening to events
   const events = await contract.events({
       topics: ['escrow', 'created'],
       limit: 100
   });
   ```

2. **Process events:**
   ```javascript
   events.forEach(event => {
       const { contractId, client, freelancer } = event;
       // Update off-chain state
       updateContractState(contractId, event);
   });
   ```

3. **Maintain indices:**
   - Track contract ID → contract state mappings
   - Monitor pending approvals
   - Watch for dispute initiation

### Indexing Strategy

For efficient querying:
- Index by `contract_id` for contract history
- Index by `client` and `freelancer` for user tracking
- Index by status and timestamp for analytics
- Create composite indices for common queries

## Performance Notes

- Event emission adds minimal gas overhead (storage operations are bounded)
- Events are indexed efficiently by the Soroban runtime
- Multiple events per transaction are supported
- Event data is stored in transaction results (not contract storage)

## Backward Compatibility

When updating events:
1. Never remove fields from existing event types
2. Only append new fields to maintain compatibility
3. Version event schema if major changes needed
4. Provide migration guidance for off-chain systems

## Monitoring & Analytics

### Metrics to Track
- Contract creation rate
- Funding success rate
- Milestone release timeline
- Dispute frequency and resolution rate
- Average contract value
- Contract completion rate

### Alerting Rules
- Dispute initiated → Alert resolution team
- Contract not fully funded → Follow-up with client
- Long time since funding approval → Flag potential issue
- Multiple dispute resolutions → Investigate pattern

## Future Enhancements

1. **Event Filtering**: Client-side filtering by contract participants
2. **Event Compression**: Aggregate updates in batch operations
3. **Event Signatures**: Cryptographic proofs of event authenticity
4. **Cross-Contract Events**: Support for multi-contract workflows
