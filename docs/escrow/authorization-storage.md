# Authorization Storage Layout and TTL Policy

This document describes the storage schema, value shapes, and time-to-live (TTL) expiration policy for authorization data in the TalentTrust escrow contract. Authorization storage includes two categories: **governance authorization** (admin roles and pending proposals) and **milestone release approvals**.

## Storage Architecture Overview

Authorization data is split between two Soroban storage layers:

| Storage Layer  | Purpose                                           | TTL     | Keys                                                            |
| -------------- | ------------------------------------------------- | ------- | --------------------------------------------------------------- |
| **Persistent** | Long-lived governance and admin state             | 30 days | `Admin`, `PendingAdmin`, `GovernedParameters`, `ProtocolFeeBps` |
| **Temporary**  | Transient approval records for milestone releases | 7 days  | `MilestoneApprovals(contract_id, milestone_index)`              |

The separation ensures that governance authorization is durable and survives node restarts, while approval records expire automatically if unused, preventing stale permissions from persisting indefinitely.

## Governance Authorization Keys

### `DataKey::Admin`

**Storage Layer**: Persistent  
**Type**: `Address`  
**Purpose**: Stores the current protocol governance administrator address.

**Value Shape**:

```rust
pub type Admin = Address; // soroban_sdk::Address
```

**Initialization**: Set by `initialize(env: Env, admin: Address)` in the contract root.

**Access Patterns**:

- Read in `set_protocol_fee_bps()` to verify caller authorization
- Read in `propose_governance_admin()` to enforce current-admin-only access
- Read via `get_governance_admin()` public query
- Updated via `finalize_governance_admin()` after timelock expires

**TTL Configuration**:

- **Initial TTL**: `PERSISTENT_TTL_LEDGERS` = 518,400 ledgers (~30 days)
- **Bump Threshold**: `PERSISTENT_BUMP_THRESHOLD` = 120,960 ledgers (~7 days)
- **Bump-on-Read**: When accessed within 7 days of expiry, TTL is extended to full 30 days

**Invariants**:

- Must be a valid Soroban address (non-zero)
- Can only be changed via the two-step admin rotation mechanism (see `PendingAdmin`)
- Must be initialized before any money-movement operations are allowed

### `DataKey::PendingAdmin`

**Storage Layer**: Persistent  
**Type**: `PendingAdminProposal`  
**Purpose**: Stores a pending governance admin proposal with timelock enforcement.

**Value Shape**:

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingAdminProposal {
    /// The address of the proposed new admin
    pub proposed: Address,
    /// The ledger sequence at which the proposal was created
    pub proposed_at_ledger: u32,
}
```

**Initialization**: None initially; created by `propose_governance_admin(proposed: Address)`.

**Access Patterns**:

- Written by `propose_governance_admin()` when current admin proposes a new admin
- Read by `finalize_governance_admin()` to check the timelock has elapsed
- Deleted by `finalize_governance_admin()` after the new admin is confirmed
- Deleted by `propose_governance_admin()` if a new proposal overwrites a pending one

**TTL Configuration**:

- **Initial TTL**: `PERSISTENT_TTL_LEDGERS` = 518,400 ledgers (~30 days)
- **Bump Threshold**: `PERSISTENT_BUMP_THRESHOLD` = 120,960 ledgers (~7 days)
- **Bump-on-Read**: Extended when read by `finalize_governance_admin()`

**Timelock Enforcement**:

- **Minimum Delay**: `ADMIN_ROTATION_MIN_DELAY_LEDGERS` = 34,560 ledgers (~2 days)
- **Enforcement**: `finalize_governance_admin()` checks `current_ledger - proposed_at_ledger >= ADMIN_ROTATION_MIN_DELAY_LEDGERS`
- **Purpose**: Allows stakeholders time to detect and react to unexpected admin changes

**Invariants**:

- Cannot be finalized until the minimum delay has elapsed
- `proposed` must differ from the current `Admin` (enforced by caller in business logic, not storage)
- Only one pending proposal can exist at a time (new proposal overwrites the previous one)

### `DataKey::ProtocolFeeBps`

**Storage Layer**: Persistent  
**Type**: `u32`  
**Purpose**: Stores the current protocol fee as basis points (bps).

**Value Shape**:

```rust
pub type ProtocolFeeBps = u32; // 0 to 10_000 inclusive, where 10_000 = 100%
```

**Range**: `0..=10_000` (enforced by `set_protocol_fee_bps()` validation)

**Initialization**: Defaults to `0` if never set.

**Access Patterns**:

- Read in `release_milestone()` to calculate protocol fee deductions
- Updated by `set_protocol_fee_bps(new_bps: u32)` (admin-gated)
- Retrieved via `get_protocol_fee_bps()` public query

**TTL Configuration**:

- **Initial TTL**: `PERSISTENT_TTL_LEDGERS` = 518,400 ledgers (~30 days)
- **Bump Threshold**: `PERSISTENT_BUMP_THRESHOLD` = 120,960 ledgers (~7 days)
- **Bump-on-Read**: Extended when accessed in money-movement paths

**Invariants**:

- Cannot exceed 10,000 bps (100%)
- Must be a non-negative integer
- Changes take effect immediately for subsequent `release_milestone()` calls

### `DataKey::GovernedParameters`

**Storage Layer**: Persistent  
**Type**: `GovernedParameters`  
**Purpose**: Stores protocol-wide governance parameters (escrow cap, future parameters).

**Value Shape**:

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernedParameters {
    /// Maximum total amount that can be held in escrow at any time (stroops)
    pub max_escrow_total_stroops: i128,
}
```

**Initialization**: Set by `set_governed_parameters()` during deployment.

**Access Patterns**:

- Read by `create_contract()` to enforce the global escrow cap
- Updated by `set_governed_parameters()` (admin-gated)
- Retrieved via `get_governed_parameters()` public query

**TTL Configuration**:

- **Initial TTL**: `PERSISTENT_TTL_LEDGERS` = 518,400 ledgers (~30 days)
- **Bump Threshold**: `PERSISTENT_BUMP_THRESHOLD` = 120,960 ledgers (~7 days)
- **Bump-on-Read**: Extended when accessed in contract-creation paths

**Invariants**:

- `max_escrow_total_stroops` must be positive (enforced by validation)
- Cannot be set to a value lower than the current total escrow amount (enforced by `set_governed_parameters()`)
- Affects only new contract creation; existing contracts are not affected

## Milestone Release Approval Keys

### `DataKey::MilestoneApprovals(contract_id, milestone_index)`

**Storage Layer**: Temporary  
**Type**: `MilestoneApprovals`  
**Purpose**: Records which parties have approved release of a specific milestone.

**Value Shape**:

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneApprovals {
    /// True if the client has approved this milestone release
    pub client_approved: bool,
    /// True if the freelancer has approved this milestone release
    pub freelancer_approved: bool,
    /// True if the arbiter has approved this milestone release
    pub arbiter_approved: bool,
}
```

**Key Construction**:

```
Key: (DataKey::MilestoneApprovals(contract_id, milestone_index))
```

Where:

- `contract_id` is a `u32` identifying the contract
- `milestone_index` is a `u32` indexing into the contract's milestone vector (0-based)

**Default State**: If no approvals record exists, it is implicitly `MilestoneApprovals { client_approved: false, freelancer_approved: false, arbiter_approved: false }`

**Initialization**: Created implicitly on first call to `approve_milestone_release()` for a given milestone.

**Access Patterns**:

1. **Write**: `approve_milestone_release(contract_id, milestone_index, caller)`
   - Creates a new approvals record if it doesn't exist
   - Sets the appropriate boolean flag based on caller identity (`client_approved`, `freelancer_approved`, or `arbiter_approved`)
   - Extends TTL if below threshold
   - Rejects duplicate approvals from the same caller (returns `AlreadyApproved` error)

2. **Read**: `release_milestone(contract_id, milestone_index, caller)`
   - Reads the approvals record to check if sufficient approvals are present
   - Validates against the contract's `release_authorization` mode (see [Authorization Matrix](#authorization-matrix))
   - Extends TTL if below threshold
   - Fails closed if record is absent or expired (treats missing as "not approved")

3. **Delete**: Implicit deletion when TTL expires after `PENDING_APPROVAL_TTL_LEDGERS` without access

**TTL Configuration**:

- **Initial TTL**: `PENDING_APPROVAL_TTL_LEDGERS` = 120,960 ledgers (~7 days)
- **Bump Threshold**: `PENDING_APPROVAL_BUMP_THRESHOLD` = 17,280 ledgers (~1 day)
- **Bump-on-Read Strategy**:
  - When `approve_milestone_release()` or `release_milestone()` reads the record
  - If remaining TTL is below 1 day, Soroban extends it back to 7 days
  - If remaining TTL is above 1 day, no extension is performed
  - This ensures active approval workflows survive the 7-day window without manual intervention

**Expiration Semantics**:

- When a record is accessed and its TTL has expired, Soroban automatically evicts it
- `read()` operations return `None` for evicted keys
- A missing or evicted record is interpreted as "not approved" (fail-closed)
- Expired approvals do NOT carry over; parties must re-approve if the entry expires

**Invariants**:

- At most one approval per party per milestone (duplicates are rejected)
- Once released, the milestone cannot be re-approved (checked before approval is recorded)
- Approvals are independent per milestone; approval of milestone `i` does not imply approval of milestone `i+1`
- Approvals are per-contract; approval in contract A does not affect contract B

## Authorization Matrix: Approval Requirements

The following table shows which approval flags must be set for each release authorization mode to allow a successful release:

| Release Authorization Mode | Required Approvals                               | Semantics                                          |
| -------------------------- | ------------------------------------------------ | -------------------------------------------------- |
| `ClientOnly`               | `client_approved == true`                        | Only client can approve; only client can release   |
| `ArbiterOnly`              | `arbiter_approved == true`                       | Only arbiter can approve; only arbiter can release |
| `ClientAndArbiter`         | `client_approved \|\| arbiter_approved == true`  | Either can approve; either can release             |
| `MultiSig`                 | `client_approved && freelancer_approved == true` | Both must approve; either can release              |

**Note on MultiSig**: In MultiSig mode, both client and freelancer must record their approval before either party can trigger a release. However, the release can be triggered by either party once both approvals are present. This differs from traditional multi-signature schemes where the signer and approver are the same entity.

## TTL Constants and Conversion

All TTL values are expressed in ledger counts. On Stellar mainnet, a new ledger is created approximately every 5 seconds.

| Constant                           | Ledger Count | Approximate Days | Purpose                                    |
| ---------------------------------- | ------------ | ---------------- | ------------------------------------------ |
| `LEDGERS_PER_DAY`                  | 17,280       | 1                | Conversion factor                          |
| `PENDING_APPROVAL_TTL_LEDGERS`     | 120,960      | 7                | Temporary storage TTL for approvals        |
| `PENDING_APPROVAL_BUMP_THRESHOLD`  | 17,280       | 1                | Threshold for extending approval TTL       |
| `PERSISTENT_TTL_LEDGERS`           | 518,400      | 30               | Persistent storage TTL for governance data |
| `PERSISTENT_BUMP_THRESHOLD`        | 120,960      | 7                | Threshold for extending governance TTL     |
| `ADMIN_ROTATION_MIN_DELAY_LEDGERS` | 34,560       | 2                | Timelock for admin proposals               |

**Note on Rounding**: Day calculations use the approximation `1 ledger ≈ 5 seconds`, which results in `17,280 ledgers per day` (exactly `1440 minutes × 60 seconds / 5 seconds per ledger`). The actual elapsed time depends on Stellar network conditions.

## Bump-on-Read Strategy

### Overview

The "bump-on-read" strategy extends the TTL of active entries when they are accessed near expiration. This ensures that:

- **Active workflows survive**: Approvals that are repeatedly accessed survive the TTL window
- **Stale entries expire**: Approvals that become dormant are eventually evicted
- **Automatic cleanup**: No manual deletion required; Soroban handles eviction

### Temporary (Approval) Entries

**Bump Threshold**: 1 day before expiry  
**Extension Behavior**:

1. When `approve_milestone_release()` or `release_milestone()` reads an approvals record
2. If the remaining TTL is below `PENDING_APPROVAL_BUMP_THRESHOLD` (1 day), Soroban extends it
3. Extension sets the new TTL to `PENDING_APPROVAL_TTL_LEDGERS` (7 days from current ledger)
4. If the remaining TTL is 1 day or more, no extension occurs

**Example Timeline**:

- Day 0: Approval recorded with TTL = 7 days → Expiry = Day 7
- Day 3: Milestone read for release check → Remaining = 4 days → No bump (above threshold)
- Day 6.5: Milestone release attempted → Remaining = 0.5 days → **Bumped** → New expiry = Day 13.5
- Day 13.5: Entry evicted if not accessed again

### Persistent (Governance) Entries

**Bump Threshold**: 7 days before expiry  
**Extension Behavior**:

1. When governance data (`Admin`, `ProtocolFeeBps`, `GovernedParameters`) is accessed
2. If remaining TTL is below `PERSISTENT_BUMP_THRESHOLD` (7 days), Soroban extends it
3. Extension sets the new TTL to `PERSISTENT_TTL_LEDGERS` (30 days from current ledger)

**Note on Governance Access Frequency**: Governance data is accessed during initialization, admin operations, and money-movement paths (fee calculations). In active contracts, this occurs frequently, so the 7-day bump threshold is rarely triggered. However, for dormant contracts or during low-activity periods, the bump ensures governance state survives the 30-day window.

## Access Patterns and Lifecycle

### Approval Lifecycle

```
1. Create Contract (no approvals initially)
   ↓
2. Approve Milestone (creates MilestoneApprovals record)
   - Record stored in temporary() with 7-day TTL
   - If accessed within 1 day of expiry, TTL bumped to 7 days
   ↓
3. Release Milestone (reads approvals, checks sufficiency)
   - If approvals sufficient, transfer funds and mark released
   - If approvals insufficient, return error
   - TTL bumped on read if near threshold
   ↓
4. (A) TTL Expires (no further access)
   - Soroban evicts the record after ~7 days
   - Subsequent reads return None (fail-closed)
   ↓
   (B) Continue Accessing (active workflow)
   - TTL extended via bump-on-read
   - Workflow continues indefinitely
```

### Governance Lifecycle

```
1. Initialize Contract (set Admin)
   - Admin stored in persistent() with 30-day TTL
   ↓
2. Normal Operations (governance data accessed frequently)
   - Admin checked during fee-gated operations
   - ProtocolFeeBps read during milestone releases
   - TTL extended via bump-on-read (7-day threshold)
   ↓
3. Admin Rotation (two-step process)
   a) Propose New Admin
      - PendingAdmin record created with current ledger
      - TTL = 30 days
      ↓
   b) Wait for Timelock (~2 days)
      ↓
   c) Finalize Admin
      - Check: (current_ledger - proposed_at_ledger) >= 34,560
      - Update: Admin = PendingAdmin.proposed
      - Delete: PendingAdmin
      - TTL reset on new Admin record
   ↓
4. Dormant Period (no access)
   - After 30 days without access, records evicted
   - Contract becomes inaccessible (archive behavior)
```

## Eviction and Recovery

### Temporary Storage Eviction

**Eviction Rule**: Soroban automatically evicts temporary entries when their TTL expires, if the entry is not renewed.

**Recovery**: Once evicted, approval records cannot be recovered. Parties must re-approve the milestone.

**Fail-Closed Semantics**: A missing or evicted record is treated as "not approved", preventing stale permissions from being honored.

### Persistent Storage Eviction

**Eviction Rule**: Soroban automatically evicts persistent entries after `PERSISTENT_TTL_LEDGERS` (30 days) if they are never accessed.

**Recovery**: Once evicted, a contract is inaccessible. The contract ID exists but cannot be read; any attempt to access it returns `ContractNotFound`.

**Archival Safety**: This is a deliberate safety measure to prevent indefinite storage bloat. Stale contracts are archived automatically after 30 days of inactivity.

## Storage Interaction with Release Authorization

The `release_authorization` field in the contract determines which approval flags must be set in the `MilestoneApprovals` record for a milestone to be released.

### Authorization Mode Details

**ClientOnly**:

- Only `client_approved` is checked
- `freelancer_approved` and `arbiter_approved` are ignored
- Only the client can call `approve_milestone_release()` and `release_milestone()`

**ArbiterOnly**:

- Only `arbiter_approved` is checked
- `client_approved` and `freelancer_approved` are ignored
- Only the arbiter can call `approve_milestone_release()` and `release_milestone()`
- Requires an arbiter to be configured in the contract

**ClientAndArbiter**:

- Either `client_approved` OR `arbiter_approved` must be true (OR logic)
- If both are true, the check passes
- Either the client or arbiter can call `approve_milestone_release()` and `release_milestone()`
- Requires an arbiter to be configured in the contract

**MultiSig**:

- Both `client_approved` AND `freelancer_approved` must be true (AND logic)
- `arbiter_approved` is ignored
- Either the client or freelancer can call `approve_milestone_release()` and `release_milestone()` after both have approved
- Arbiter is optional (not required for MultiSig mode)

## Cross-References

- **Authorization Matrix and Workflow**: See [docs/escrow/authorization.md](authorization.md) for approval and release semantics.
- **TTL Implementation**: See [contracts/escrow/src/ttl.rs](../../contracts/escrow/src/ttl.rs) for TTL constants and helper functions.
- **Governance Module**: See [contracts/escrow/src/governance.rs](../../contracts/escrow/src/governance.rs) for admin and protocol-fee entrypoints.
- **Approvals Module**: See [contracts/escrow/src/approvals.rs](../../contracts/escrow/src/approvals.rs) for milestone approval recording and validation.
- **Contract Types**: See [contracts/escrow/src/types.rs](../../contracts/escrow/src/types.rs) for `DataKey`, `MilestoneApprovals`, `PendingAdminProposal`, and other type definitions.

## Key Takeaways

1. **Governance authorization** (admin roles) is stored persistently with 30-day TTL and 2-day admin rotation timelock.
2. **Milestone approvals** are stored temporarily with 7-day TTL and bump-on-read strategy for active workflows.
3. **Bump thresholds** (1 day for approvals, 7 days for governance) ensure entries are renewed when actively used but expire if dormant.
4. **Fail-closed semantics**: Missing or expired records are treated as "not approved", preventing stale permissions.
5. **Authorization matrix** determines which approval flags are required based on the contract's `release_authorization` mode.
6. **Automatic eviction** prevents indefinite storage bloat; stale contracts are archived after 30 days of inactivity.
