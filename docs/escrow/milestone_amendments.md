# Milestone Amendment Process

This document describes the process for amending unreleased milestones in the Talenttrust Escrow contract.

## Overview

The milestone amendment process allows either the client or the freelancer to propose a new amount for a milestone that has not yet been released. For the amendment to take effect, the other party must approve it.

## Flow

1.  **Proposal**: Either the Client or the Freelancer calls `propose_milestone_amendment`.
    - This creates a pending amendment for the specific milestone.
    - Only one pending amendment can exist per milestone; a new proposal will overwrite the previous one.
2.  **Approval/Rejection**:
    - The other party (who did not propose) calls `approve_milestone_amendment` to accept the change.
    - The milestone amount is updated, and the proposal is cleared.
    - Either party can call `reject_milestone_amendment` to clear the proposal without changing the milestone.

## Security Considerations

- **Authorization**: Only the client and the freelancer of the specific escrow contract are authorized to propose, approve, or reject amendments.
- **State Constraint**: Amendments can only be proposed and approved for milestones that are NOT yet released.
- **Mutual Agreement**: The party that proposed the amendment CANNOT approve it. High-security escrow requires both parties to agree on value changes.
- **Contract State**: The contract must be in the `Funded` state to allow amendments (as `Created` state is for initial setup and `Completed` is final).

## Technical Implementation

### Data Structures

```rust
#[contracttype]
pub struct Amendment {
    pub new_amount: i128,
    pub proposer: Address,
    pub created_at: u64,
}
```

### Storage

Amendments are stored in persistent storage using the key:
`DataKey::Amendment(contract_id, milestone_id)`

### Functions

- `propose_milestone_amendment(contract_id, milestone_id, proposer, new_amount)`
- `approve_milestone_amendment(contract_id, milestone_id, approver)`
- `reject_milestone_amendment(contract_id, milestone_id, caller)`
- `get_amendment(contract_id, milestone_id)`
