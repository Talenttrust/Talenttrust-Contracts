# Escrow Contract: Milestone Validation and Approval Flow

## Overview
This document describes the milestone validation logic and approval flow implemented in the escrow smart contract for the TalentTrust protocol.

## Approval Flow Architecture

### Milestone Approvals
The contract implements a flexible approval system that supports multiple authorization modes:

1. **ClientOnly**: Only the client can approve milestone releases
2. **ArbiterOnly**: Only the arbiter can approve milestone releases
3. **ClientAndArbiter**: Either the client or arbiter can approve (OR logic)
4. **MultiSig**: Both client and freelancer must approve (AND logic)

### Approval Storage and TTL
Approvals are stored in **temporary storage** with automatic expiry:
- **TTL**: `PENDING_APPROVAL_TTL_LEDGERS` (120,960 ledgers ≈ 7 days at 5 sec/ledger)
- **Bump Threshold**: `PENDING_APPROVAL_BUMP_THRESHOLD` (60,480 ledgers ≈ 3.5 days)
- **Minimum TTL**: `MIN_APPROVAL_TTL` (17,280 ledgers ≈ 1 day)

Expired approvals are automatically evicted by Soroban's temporary storage and treated as absent.

### Approval Process

#### 1. Approve Milestone (`approve_milestone_release`)
```rust
pub fn approve_milestone_release(
    env: Env,
    contract_id: u32,
    caller: Address,
    milestone_index: u32,
) -> bool
```

**Requirements:**
- Contract must be in `Funded` state
- Milestone must not be already released
- Caller must be authorized based on `ReleaseAuthorization` mode
- Caller must not have already approved this milestone

**Behavior:**
- Records approval in temporary storage with TTL
- Prevents duplicate approvals from the same party
- Stores `MilestoneApprovals` struct with flags for client/freelancer/arbiter

#### 2. Release Milestone (`release_milestone`)
```rust
pub fn release_milestone(
    env: Env,
    contract_id: u32,
    caller: Address,
    milestone_index: u32,
) -> bool
```

**Requirements:**
- Contract must be in `Funded` state
- Valid, non-expired approvals must exist
- Sufficient approvals based on authorization mode
- Milestone must not be already released or refunded
- Sufficient funds must be available

**Behavior:**
- Checks for valid approvals via `check_approvals()`
- Marks milestone as released
- Updates contract accounting
- **Clears approvals** after successful release (prevents reuse)
- Transitions to `Completed` status if all milestones are released

## Validation Rules

### Contract Creation
- **Non-empty milestones**: At least one milestone must be provided
- **Positive amounts**: All milestone amounts must be strictly positive (> 0)
- **Distinct participants**: Client and freelancer must be different addresses
- **Arbiter validation**: 
  - Required for `ArbiterOnly` and `ClientAndArbiter` modes
  - Must be different from client and freelancer

### Approval Validation
- **State check**: Contract must be in `Funded` state
- **Index bounds**: Milestone index must be valid
- **Not released**: Milestone must not be already released
- **Authorization**: Caller must be authorized for the contract's release mode
- **No duplicates**: Same party cannot approve twice

### Release Validation
- **Approval check**: Required approvals must exist and not be expired
- **State check**: Contract must be in `Funded` state
- **Not released**: Milestone must not be already released
- **Not refunded**: Milestone must not be already refunded
- **Sufficient funds**: Contract must have enough balance

## Security Assumptions

### Fail-Closed Design
- Missing approvals → release fails
- Expired approvals → release fails (treated as absent)
- Insufficient approvals → release fails
- Invalid state → operation fails

### Authorization Enforcement
- All operations require `caller.require_auth()`
- Role-based access control enforced at approval and release
- Arbiter cannot be client or freelancer (prevents role overlap)

### Storage Security
- Approvals use temporary storage with TTL
- Automatic expiry prevents stale approvals
- Approvals cleared after successful release (prevents reuse)
- TTL bump threshold ensures approvals don't expire unexpectedly

### Accounting Integrity
- Available balance checked before release
- Released/refunded amounts tracked separately
- Overflow protection via i128 arithmetic
- State transitions are atomic

## Threat Scenarios

### Prevented Attacks
1. **Replay attacks**: Approvals cleared after use, expired approvals rejected
2. **Unauthorized releases**: Role-based authorization enforced
3. **Stale approvals**: TTL expiry automatically invalidates old approvals
4. **Double-spending**: Released/refunded flags prevent duplicate operations
5. **Role confusion**: Arbiter validation prevents overlap with client/freelancer

### Mitigations
- **Approval expiry**: Prevents indefinite approval validity
- **Duplicate prevention**: Same party cannot approve twice
- **State machine**: Strict status transitions prevent invalid operations
- **Balance checks**: Prevents over-release of funds

## Test Coverage

### Unit Tests (`approvals.rs`)
- Approval recording with different authorization modes
- Duplicate approval rejection
- Unauthorized approval rejection
- Approval expiry behavior

### Integration Tests (`test/approval_expiry.rs`)
- ClientOnly mode approval and release
- MultiSig mode requiring both approvals
- ArbiterOnly mode enforcement
- ClientAndArbiter OR logic
- Release without approval fails
- Release with approval succeeds
- Approval clearing after release
- Multiple independent milestone approvals
- Invalid state/index handling

### Edge Cases Covered
- Expired approvals (TTL elapsed)
- Insufficient approvals for release
- Already released milestone approval attempt
- Invalid milestone index
- Unfunded contract approval attempt
- Multiple milestones with independent approvals

## Future Improvements
- Partial approval revocation mechanism
- Approval delegation/proxy support
- Time-locked approvals with minimum wait period
- Approval event emission for off-chain tracking
- Batch approval operations for multiple milestones
