# Release Authorization and Approval Lifecycle

This document provides an authoritative guide to the escrow contract's release authorization modes and the approval-then-release flow. It defines who may approve milestones, who may trigger releases, how many approvals each mode requires, and how TTL-based approval expiry interacts with release operations.

## Overview

The escrow contract supports four `ReleaseAuthorization` modes that control who can approve milestone releases and who can execute the release transaction. These modes are defined in `contracts/escrow/src/types.rs` and enforced across `contracts/escrow/src/approvals.rs` and `release_milestone` in `contracts/escrow/src/lib.rs`.

## ReleaseAuthorization Modes

The four authorization modes are:

| Mode | Enum Value | Description |
|------|------------|-------------|
| `ClientOnly` | 0 | Only the client can approve and release |
| `ClientAndArbiter` | 1 | Either the client or arbiter can approve and release |
| `ArbiterOnly` | 2 | Only the arbiter can approve and release |
| `MultiSig` | 3 | Both client and freelancer must approve; either can release |

## Authorization Matrix

Per-mode authorization rules for approval and release operations:

### ClientOnly Mode

| Aspect | Rule |
|--------|------|
| **Allowed Approvers** | Client only |
| **Required Approvals** | Client approval (1 signature) |
| **Allowed Release Callers** | Client only |
| **Approval Check Logic** | `approvals.client_approved` must be `true` |
| **Failure Error Codes** | `UnauthorizedRole` (if non-client attempts), `AlreadyApproved` (duplicate), `InsufficientApprovals` (missing) |

### ArbiterOnly Mode

| Aspect | Rule |
|--------|------|
| **Allowed Approvers** | Arbiter only |
| **Required Approvals** | Arbiter approval (1 signature) |
| **Allowed Release Callers** | Arbiter only |
| **Approval Check Logic** | `approvals.arbiter_approved` must be `true` |
| **Failure Error Codes** | `UnauthorizedRole` (if non-arbiter attempts), `AlreadyApproved` (duplicate), `InsufficientApprovals` (missing) |
| **Contract Creation Requirement** | Arbiter must be provided (enforced by `MissingArbiter` error) |

### ClientAndArbiter Mode

| Aspect | Rule |
|--------|------|
| **Allowed Approvers** | Client OR Arbiter |
| **Required Approvals** | Either client OR arbiter approval (1 signature, OR logic) |
| **Allowed Release Callers** | Client OR Arbiter |
| **Approval Check Logic** | `approvals.client_approved || approvals.arbiter_approved` must be `true` |
| **Failure Error Codes** | `UnauthorizedRole` (if freelancer attempts), `AlreadyApproved` (duplicate from same party), `InsufficientApprovals` (neither approved) |
| **Contract Creation Requirement** | Arbiter must be provided (enforced by `MissingArbiter` error) |

### MultiSig Mode

| Aspect | Rule |
|--------|------|
| **Allowed Approvers** | Client AND Freelancer |
| **Required Approvals** | Both client AND freelancer approval (2 signatures, AND logic) |
| **Allowed Release Callers** | Client OR Freelancer (either can trigger release after both approve) |
| **Approval Check Logic** | `approvals.client_approved && approvals.freelancer_approved` must be `true` |
| **Failure Error Codes** | `UnauthorizedRole` (if arbiter attempts), `AlreadyApproved` (duplicate from same party), `InsufficientApprovals` (one or both missing) |
| **Contract Creation Requirement** | Arbiter optional (not required) |

**Note on MultiSig Inconsistency**: The MultiSig mode requires both client and freelancer to approve, but allows either party to trigger the release. This differs from the typical multi-signature pattern where approval and release are the same operation. The current implementation separates approval (recording intent) from release (executing the transfer), which enables the release caller to be different from the approvers.

## Approval Lifecycle

The approval-then-release flow follows this sequence:

### 1. Approve Milestone (`approve_milestone_release`)

**Entry Point**: `contracts/escrow/src/lib.rs::approve_milestone_release` → `contracts/escrow/src/approvals.rs::approve_milestone`

**Purpose**: Records a party's approval for a specific milestone release.

**Prerequisites**:
- Contract must exist and be in `Funded` state
- Milestone index must be valid
- Milestone must not already be released
- Caller must be authenticated via `require_auth()`
- Caller must be authorized based on the `ReleaseAuthorization` mode

**Storage**: Approvals are stored in temporary storage under `DataKey::MilestoneApprovals(contract_id, milestone_index)` with the following structure:
```rust
pub struct MilestoneApprovals {
    pub client_approved: bool,
    pub freelancer_approved: bool,
    pub arbiter_approved: bool,
}
```

**TTL Configuration**:
- Initial TTL: `PENDING_APPROVAL_TTL_LEDGERS` = 120,960 ledgers (~7 days at ~5s per ledger)
- Bump threshold: `PENDING_APPROVAL_BUMP_THRESHOLD` = 17,280 ledgers (~1 day)
- TTL is extended to the full `PENDING_APPROVAL_TTL_LEDGERS` whenever the entry is accessed above the bump threshold

**Error Codes**:
- `ContractNotFound`: Contract does not exist
- `InvalidState`: Contract not in `Funded` state
- `IndexOutOfBounds`: Milestone index invalid
- `MilestoneAlreadyReleased`: Milestone already released
- `UnauthorizedRole`: Caller not authorized to approve for this mode
- `AlreadyApproved`: Caller already approved this milestone

**Security Properties**:
- Caller authentication enforced via `require_auth()`
- Duplicate approvals from the same party are rejected
- Approvals auto-expire after TTL elapses (Soroban temporary storage eviction)
- Fail-closed: missing or expired approvals prevent release

### 2. Check Approvals (`check_approvals`)

**Entry Point**: `contracts/escrow/src/approvals.rs::check_approvals`

**Purpose**: Validates that sufficient approvals exist for a milestone release.

**Behavior**:
- Loads approvals from temporary storage
- Returns `None` if approvals don't exist or have expired (TTL elapsed)
- Checks approval sufficiency based on `ReleaseAuthorization` mode

**Approval Sufficiency Logic**:
```rust
match contract.release_authorization {
    ReleaseAuthorization::ClientOnly => approvals.client_approved,
    ReleaseAuthorization::ArbiterOnly => approvals.arbiter_approved,
    ReleaseAuthorization::ClientAndArbiter => {
        approvals.client_approved || approvals.arbiter_approved
    }
    ReleaseAuthorization::MultiSig => {
        approvals.client_approved && approvals.freelancer_approved
    }
}
```

**Error Codes**:
- `InsufficientApprovals`: Approvals missing, insufficient, or expired

**Security Properties**:
- Fail-closed: expired approvals are treated as absent
- TTL expiry is enforced by Soroban's temporary storage (automatic eviction)

### 3. Release Milestone (`release_milestone`)

**Entry Point**: `contracts/escrow/src/lib.rs::release_milestone`

**Purpose**: Executes the fund transfer to the freelancer for a specific milestone.

**Prerequisites**:
- Contract must exist and be in `Funded` state
- Caller must be authenticated via `require_auth()`
- Caller must be authorized to release based on the `ReleaseAuthorization` mode
- Valid, non-expired approvals must exist (checked via `check_approvals`)
- Milestone must not already be released or refunded
- Sufficient funds must be available

**Release Authorization Check**:
```rust
match contract.release_authorization {
    ReleaseAuthorization::ClientOnly => {
        if !is_client { return Err(Error::UnauthorizedRole); }
    }
    ReleaseAuthorization::ArbiterOnly => {
        if !is_arbiter { return Err(Error::UnauthorizedRole); }
    }
    ReleaseAuthorization::ClientAndArbiter => {
        if !is_client && !is_arbiter { return Err(Error::UnauthorizedRole); }
    }
    ReleaseAuthorization::MultiSig => {
        if !is_client && !is_freelancer { return Err(Error::UnauthorizedRole); }
    }
}
```

**Error Codes**:
- `ContractNotFound`: Contract does not exist
- `InvalidState`: Contract not in `Funded` state
- `IndexOutOfBounds`: Milestone index invalid
- `MilestoneAlreadyReleased`: Milestone already released
- `AlreadyRefunded`: Milestone already refunded
- `InsufficientFunds`: Insufficient contract balance
- `InsufficientApprovals`: Required approvals missing or expired
- `UnauthorizedRole`: Caller not authorized to release for this mode

**Side Effects**:
- Transfers milestone amount to freelancer
- Marks milestone as released
- Updates contract `released_amount`
- Accumulates protocol fees if configured
- Transitions contract to `Completed` if all milestones released/refunded
- Clears approval records (see below)

### 4. Clear Approvals (`clear_approvals`)

**Entry Point**: `contracts/escrow/src/approvals.rs::clear_approvals`

**Purpose**: Removes approval records after successful release to prevent reuse.

**Behavior**:
- Removes the `MilestoneApprovals` entry from temporary storage
- Called automatically after successful `release_milestone`

**Security Properties**:
- Prevents approval reuse across multiple releases
- Cleans up temporary storage
- Idempotent: safe to call multiple times

## TTL Expiry Behavior

### Pending Approval TTL

Pending approvals are stored in Soroban's temporary storage with a time-to-live (TTL) policy defined in `contracts/escrow/src/ttl.rs`:

**Constants**:
```rust
pub const LEDGERS_PER_DAY: u32 = 17_280;
pub const PENDING_APPROVAL_TTL_LEDGERS: u32 = LEDGERS_PER_DAY * 7;  // ~7 days
pub const PENDING_APPROVAL_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY;   // ~1 day
```

**Expiry Window**: 120,960 ledgers (~7 days at ~5s per ledger on mainnet)

**TTL Extension Logic**:
- When an approval is recorded, TTL is set to `PENDING_APPROVAL_TTL_LEDGERS`
- When the approval entry is accessed above the bump threshold, TTL is extended back to the full `PENDING_APPROVAL_TTL_LEDGERS`
- If the entry is not accessed before TTL elapses, Soroban auto-evicts it

**Interaction with Release**:
- Expired approvals are indistinguishable from never-set approvals (both return `None`)
- `check_approvals` treats expired approvals as insufficient and returns `InsufficientApprovals`
- This provides a fail-closed security property: expired approvals cannot be used to release funds

**Recovery from Expiry**:
- If approvals expire, all parties must re-approve the milestone
- This prevents stale approvals from being used long after they were granted
- Integrators should monitor approval TTL and re-approve before expiry if needed

## Complete Flow Example

### ClientOnly Mode Flow

1. **Client calls** `approve_milestone_release(contract_id, client_address, milestone_index)`
   - Approval recorded: `client_approved = true`
   - TTL set to 7 days

2. **Client calls** `release_milestone(contract_id, client_address, milestone_index)`
   - Caller authorization check: client is authorized ✓
   - Approval check: `client_approved = true` ✓
   - Funds transferred to freelancer
   - Approval cleared

### MultiSig Mode Flow

1. **Client calls** `approve_milestone_release(contract_id, client_address, milestone_index)`
   - Approval recorded: `client_approved = true`
   - TTL set to 7 days
   - Approval check fails: `client_approved && freelancer_approved = false` → `InsufficientApprovals`

2. **Freelancer calls** `approve_milestone_release(contract_id, freelancer_address, milestone_index)`
   - Approval recorded: `freelancer_approved = true`
   - TTL extended to 7 days
   - Approval check passes: `client_approved && freelancer_approved = true` ✓

3. **Either client or freelancer calls** `release_milestone(contract_id, caller_address, milestone_index)`
   - Caller authorization check: client or freelancer is authorized ✓
   - Approval check: both approved ✓
   - Funds transferred to freelancer
   - Approval cleared

## Error Code Reference

| Error Code | Value | When Raised |
|------------|-------|-------------|
| `UnauthorizedRole` | 10, 11 | Caller not authorized for approval or release in current mode |
| `AlreadyApproved` | 18 | Caller already approved this milestone (duplicate approval) |
| `InsufficientApprovals` | 19, 20 | Required approvals missing, insufficient, or expired |
| `MissingArbiter` | 2 | Arbiter required but not provided (ArbiterOnly or ClientAndArbiter modes) |
| `InvalidArbiter` | 3 | Arbiter is same as client or freelancer |
| `ContractNotFound` | 9 | Contract does not exist |
| `InvalidState` | 11, 16 | Contract not in `Funded` state |
| `IndexOutOfBounds` | 3, 12 | Milestone index invalid |
| `MilestoneAlreadyReleased` | 13, 17 | Milestone already released |

## Security Considerations

### Fail-Closed Design

- Missing approvals prevent release (`InsufficientApprovals`)
- Expired approvals prevent release (treated as missing)
- Unauthorized callers are rejected (`UnauthorizedRole`)
- Duplicate approvals are rejected (`AlreadyApproved`)

### Authentication

- All approval and release operations require `require_auth()`
- Soroban's native authentication ensures the caller is who they claim to be

### Approval Isolation

- Approvals are stored per-milestone, not per-contract
- Clearing approvals after release prevents reuse
- TTL expiry prevents stale approvals from being used

### Mode-Specific Guarantees

- **ClientOnly**: Only client can approve/release, ensuring client control
- **ArbiterOnly**: Only arbiter can approve/release, enabling dispute resolution
- **ClientAndArbiter**: Either can approve/release, providing flexibility
- **MultiSig**: Both must approve, ensuring mutual agreement before release

## Implementation References

- **Type definitions**: `contracts/escrow/src/types.rs` (ReleaseAuthorization enum, MilestoneApprovals struct)
- **Approval logic**: `contracts/escrow/src/approvals.rs` (approve_milestone, check_approvals, clear_approvals)
- **Release logic**: `contracts/escrow/src/lib.rs` (release_milestone, approve_milestone_release)
- **TTL configuration**: `contracts/escrow/src/ttl.rs` (PENDING_APPROVAL_TTL_LEDGERS, PENDING_APPROVAL_BUMP_THRESHOLD)

## Testing Coverage

The authorization modes are tested in:
- `contracts/escrow/src/approvals.rs` (unit tests for approval logic)
- `contracts/escrow/src/test/flows.rs` (integration tests for complete flows)
- `contracts/escrow/src/test/security.rs` (security-focused tests for authorization)

Test coverage ensures:
- Each mode enforces the correct approver set
- Each mode enforces the correct release caller set
- TTL expiry prevents release with expired approvals
- Duplicate approvals are rejected
- Unauthorized callers are rejected
- Approval clearing works correctly

## NatSpec Cross-References

The following NatSpec comments in the source code provide additional context:

- `/// Defines who can approve milestone releases.` in `types.rs` (ReleaseAuthorization enum)
- `/// Approves a milestone for release by the caller.` in `approvals.rs` (approve_milestone)
- `/// Checks if a milestone has sufficient approvals for release.` in `approvals.rs` (check_approvals)
- `/// Clears approval records for a milestone after successful release.` in `approvals.rs` (clear_approvals)
- `/// Approves a milestone for release.` in `lib.rs` (approve_milestone_release)
- `/// Releases a specific milestone, transferring funds to the freelancer.` in `lib.rs` (release_milestone)
