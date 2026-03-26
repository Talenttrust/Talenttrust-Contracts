# Escrow Contract Documentation

## Overview

The TalentTrust Escrow contract provides a decentralized escrow system for freelancer-client relationships with built-in dispute resolution capabilities. Built on Soroban (Stellar), it ensures secure fund management and fair dispute resolution.

## Architecture

### Core Components

1. **EscrowContract**: Main contract structure storing client, freelancer, milestones, and status
2. **Dispute**: Dispute tracking with evidence, resolution type, and payout amounts
3. **Access Control**: Role-based permissions for admin, arbitrator, client, and freelancer

### Storage Structure

```
├── ADMIN: Address           # Contract administrator
├── ARBITRATOR: Address     # Dispute resolver
├── CONTRACTS: Map<u32, EscrowContract>
├── DISPUTES: Map<u32, Dispute>
├── NEXT_CONTRACT_ID: u32
└── NEXT_DISPUTE_ID: u32
```

## Contract States

### ContractStatus
- `Created`: Contract created, awaiting funding
- `Funded`: Funds deposited, milestones available for release
- `Completed`: All milestones released successfully
- `Disputed`: Dispute opened, contract paused
- `Resolved`: Dispute resolved, payouts processed
- `Cancelled`: Contract cancelled (future feature)

### DisputeStatus
- `Open`: Dispute created, awaiting review
- `InReview`: Dispute being reviewed by arbitrator
- `Resolved`: Dispute resolved with payouts determined

## Functions

### Initialization

#### `initialize(admin: Address, arbitrator: Address)`
- **Purpose**: Initialize contract with admin and arbitrator addresses
- **Access**: Anyone (but requires admin signature)
- **Security**: Prevents re-initialization

### Contract Management

#### `create_contract(client: Address, freelancer: Address, milestone_amounts: Vec<i128>) -> u32`
- **Purpose**: Create new escrow contract with milestone payments
- **Access**: Client only
- **Returns**: Unique contract ID

#### `deposit_funds(contract_id: u32, amount: i128) -> bool`
- **Purpose**: Deposit total contract amount into escrow
- **Access**: Client only
- **Validation**: Amount must equal total milestone amounts

#### `release_milestone(contract_id: u32, milestone_id: u32) -> bool`
- **Purpose**: Release specific milestone payment to freelancer
- **Access**: Client only
- **Validation**: Milestone must exist and not be previously released

### Dispute Resolution

#### `create_dispute(contract_id: u32, reason: Symbol, evidence: Vec<Symbol>) -> u32`
- **Purpose**: Create dispute for funded contract
- **Access**: Client or Freelancer only
- **Returns**: Unique dispute ID
- **Effect**: Contract status changes to `Disputed`

#### `resolve_dispute(dispute_id: u32, resolution: DisputeResolution, client_payout: i128, freelancer_payout: i128) -> bool`
- **Purpose**: Resolve dispute with specific outcome
- **Access**: Arbitrator only
- **Resolution Types**:
  - `FullRefund`: 100% to client
  - `PartialRefund`: 70% to client, 30% to freelancer
  - `FullPayout`: 100% to freelancer
  - `Split`: Custom amounts (must total contract amount)

### Admin Functions

#### `update_admin(new_admin: Address)`
- **Purpose**: Update admin address
- **Access**: Current admin only

#### `update_arbitrator(new_arbitrator: Address)`
- **Purpose**: Update arbitrator address
- **Access**: Admin only

## Security Features

### Access Control
- **Admin**: Can update arbitrator, manage contract settings
- **Arbitrator**: Can resolve disputes, determine payouts
- **Client**: Can create contracts, deposit funds, release milestones, create disputes
- **Freelancer**: Can create disputes, receive milestone payments

### Validation Rules
1. Contract must be in correct state for operations
2. Financial amounts must be mathematically valid
3. Only authorized parties can perform actions
4. Dispute resolution payouts are deterministic

### Threat Mitigation
- **Unauthorized access**: Role-based authentication
- **Invalid payouts**: Mathematical validation of splits
- **Double spending**: State machine prevents invalid transitions
- **Front-running**: Timestamp tracking for dispute resolution

## Usage Examples

### Basic Workflow

```rust
// 1. Initialize contract
escrow.initialize(admin_address, arbitrator_address);

// 2. Create contract
let contract_id = escrow.create_contract(
    client_address,
    freelancer_address,
    vec![1000_0000000, 2000_0000000] // Milestones in stroops
);

// 3. Deposit funds
escrow.deposit_funds(contract_id, 3000_0000000);

// 4. Release milestone
escrow.release_milestone(contract_id, 0); // First milestone
```

### Dispute Resolution

```rust
// 5. Create dispute (when issues arise)
let dispute_id = escrow.create_dispute(
    contract_id,
    symbol_short!("quality_issues"),
    vec![symbol_short!("evidence1"), symbol_short!("evidence2")]
);

// 6. Resolve dispute (arbitrator only)
escrow.resolve_dispute(
    dispute_id,
    DisputeResolution::PartialRefund,
    0,  // Not used for PartialRefund
    0   // Not used for PartialRefund
);
```

## Testing

The contract includes comprehensive tests covering:
- Normal workflow operations
- All dispute resolution scenarios
- Access control violations
- Edge cases and error conditions
- Security validation

Run tests with:
```bash
cargo test
```

## Future Enhancements

- Reputation system integration
- Multi-signature dispute resolution
- Time-based escrow releases
- Gas optimization for high-volume usage
- Cross-chain dispute resolution
This document describes escrow-specific controls and operational guidance.

## Emergency Pause Controls

The escrow contract includes admin-managed incident response controls:

- `initialize(admin)`: Sets the admin address once.
- `pause()`: Temporarily pauses state-changing functions.
- `unpause()`: Re-enables operations after a normal pause.
- `activate_emergency_pause()`: Activates emergency mode and hard-pauses operations.
- `resolve_emergency()`: Clears emergency mode and unpauses the contract.
- `is_paused()`: Read-only pause status.
- `is_emergency()`: Read-only emergency status.

### Guarded Functions

While paused, these state-changing flows revert with `ContractPaused`:

- `create_contract`
- `deposit_funds`
- `release_milestone`
- `issue_reputation`

### Error Codes

- `1` `AlreadyInitialized`
- `2` `NotInitialized`
- `3` `ContractPaused`
- `4` `NotPaused`
- `5` `EmergencyActive`

## Security Notes

- Admin-only controls: pause and emergency operations require authenticated admin.
- One-time initialization: admin cannot be replaced accidentally by repeated init calls.
- Emergency lock discipline: `unpause` is blocked while emergency mode is active.
- Fail-closed behavior: guarded functions revert whenever `paused == true`.

## Operational Playbook

1. Detect incident and call `activate_emergency_pause`.
2. Investigate and remediate root cause.
3. Validate mitigations in test/staging.
4. Call `resolve_emergency` to restore service.
5. Publish incident summary for ecosystem transparency.
