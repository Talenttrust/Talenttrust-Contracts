# Storage Layout and TTL Policy

## Current Status

The escrow contract (`contracts/escrow/src/lib.rs`) is currently in a skeleton implementation phase. Persistent storage is not yet implemented - all functions return placeholder values. The comments in the code indicate:

> "Full implementation would store state in persistent storage."

This document describes the **intended** storage layout and TTL/bump strategy based on the contract structure and Soroban best practices.

## Intended Storage Layout

### Storage Keys

The following storagekeys are planned for the escrow contract:

#### Contract Data

- **Key**: `Symbol::from_short("Contract")` or similar
- **Value**: Struct containing:
  - `client: Address` - The client who funds the escrow
  - `freelancer: Address` - The freelancer who receives payments
  - `status: ContractStatus` - Current contract state (Created, Funded, Completed, Disputed)
  - `milestones: Vec<Milestone>` - Array of milestone payment structures

#### Milestone Data

- **Key**: `Symbol::from_short("Milestones")` or similar
- **Value**: `Vec<Milestone>` where each `Milestone` contains:
  - `amount: i128` - Payment amount for the milestone (in stroops)
  - `released: bool` - Whether the milestone has been released to the freelancer

#### Reputation Data

- **Key**: `Symbol::from_short("Reputation")` or similar
- **Value**: Struct containing:
  - `freelancer: Address` - The freelancer's address
  - `rating: i128` - Reputation rating issued after contract completion

### Data Types

#### ContractStatus Enum

```rust
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContractStatus {
    Created = 0,
    Funded = 1,
    Completed = 2,
    Disputed = 3,
}
```

#### Milestone Struct

```rust
#[contracttype]
#[derive(Clone, Debug)]
pub struct Milestone {
    pub amount: i128,
    pub released: bool,
}
```

## TTL/Bump Strategy

### Soroban Storage TTL Overview

Soroban uses a Time-To-Live (TTL) system for storage entries. Each storage entry has a lifetime that must be periodically extended ("bumped") to prevent eviction.

### Recommended TTL Strategy

#### Contract Instance TTL

- **Initial TTL**: 518,400 ledgers (~72 hours at ~5 second ledger time)
- **Bump Strategy**: Bump on every contract invocation
- **Implementation**: Use `env.storage().instance().extend_ttl()` in each public function

#### Storage Entry TTL

- **Initial TTL**: 518,400 ledgers (~72 hours)
- **Bump Strategy**: Bump storage entries when:
  - Contract is created
  - Funds are deposited
  - Milestones are released
  - Status changes
- **Implementation**: Use `env.storage().persistent().extend_ttl()` for each storage key

### Example Bump Implementation

```rust
// At the start of each public function
env.storage().instance().extend_ttl(100, 518_400);

// After writing to storage
env.storage().persistent().extend_ttl(&key, 100, 518_400);
```

### Bump Parameters

- **threshold_ledgers**: 100 - Bump when TTL is below this threshold
- **extend_to**: 518,400 - Extend TTL to this many ledgers (~72 hours)

## Cross-Reference to Code

### Contract Creation

**Function**: `create_contract` (line 28-37 in `contracts/escrow/src/lib.rs`)

**Intended Storage Operations**:
- Store client address
- Store freelancer address
- Store contract status as `ContractStatus::Created`
- Store milestone amounts as `Vec<Milestone>`
- Bump instance TTL
- Bump storage entry TTLs

### Fund Deposit

**Function**: `deposit_funds` (line 39-43 in `contracts/escrow/src/lib.rs`)

**Intended Storage Operations**:
- Update contract status to `ContractStatus::Funded`
- Bump instance TTL
- Bump storage entry TTLs

### Milestone Release

**Function**: `release_milestone` (line 45-49 in `contracts/escrow/src/lib.rs`)

**Intended Storage Operations**:
- Update specific milestone `released` flag to `true`
- Bump instance TTL
- Bump storage entry TTLs

### Reputation Issuance

**Function**: `issue_reputation` (line 51-55 in `contracts/escrow/src/lib.rs`)

**Intended Storage Operations**:
- Store reputation credential for freelancer
- Update contract status to `ContractStatus::Completed` (if last milestone)
- Bump instance TTL
- Bump storage entry TTLs

## Implementation Notes

1. **Storage Access Control**: Ensure only authorized parties (client for deposits, authorized party for milestone releases) can modify storage entries.

2. **Atomic Operations**: Use Soroban's atomic transaction capabilities to ensure storage updates are consistent.

3. **Error Handling**: Implement proper error handling for storage operations (e.g., entry not found, insufficient permissions).

4. **Gas Optimization**: Consider storage access patterns to minimize gas costs - batch reads/writes where possible.

## References

- Soroban SDK Documentation: https://docs.soroban.stellar.org/
- Soroban Storage: https://docs.soroban.stellar.org/docs/learn/storage
- Contract Code: `contracts/escrow/src/lib.rs`
