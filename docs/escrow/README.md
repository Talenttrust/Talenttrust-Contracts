# Escrow Contract Specification

## Overview

The **Escrow** contract provides a secure, milestone-based payment system for freelancer engagements on the Stellar (Soroban) network. It ensures that:

- Funds are held in escrow by a trusted contract (not released early).
- Payments are tied to specific milestones completed by the freelancer.
- Client approval is required before any payment release.
- Reputation credentials are issued **only after** successful contract completion and final settlement.

## Contract Architecture

### Internal State

The contract uses Soroban's persistent storage with the following keys:

```rust
pub enum DataKey {
    Contract(u32),           // Full EscrowContract keyed by numeric ID
    ReputationIssued(u32),   // Boolean flag: has reputation been issued for this contract?
    NextId,                  // Auto-incrementing contract ID counter (starts at 1)
}
```

### Data Structures

#### `ContractStatus`

An enumeration representing the contract lifecycle:

| Status | Value | Meaning |
|--------|-------|---------|
| `Created` | 0 | Contract created; awaiting client deposit |
| `Funded` | 1 | Client has deposited funds; work is in progress |
| `Completed` | 2 | All milestones released; contract finalised |
| `Disputed` | 3 | A dispute has been raised (payments paused) |

#### `Milestone`

```rust
pub struct Milestone {
    pub amount: i128,      // Payment in stroops (1 XLM = 10_000_000 stroops)
    pub released: bool,    // Has the client released this payment?
}
```

#### `EscrowContract`

```rust
pub struct EscrowContract {
    pub client: Address,              // Party commissioning work & funding escrow
    pub freelancer: Address,          // Party performing work & receiving payments
    pub milestones: Vec<Milestone>,   // Ordered list of deliverables & payments
    pub status: ContractStatus,       // Current lifecycle state
}
```

## Function Reference

### `create_contract`

**Signature:**
```rust
pub fn create_contract(
    env: Env,
    client: Address,
    freelancer: Address,
    milestone_amounts: Vec<i128>,
) -> u32
```

**Description:**
Creates a new escrow engagement and assigns it a unique numeric ID.

**Behavior:**
- Initializes all milestones with `released = false`.
- Sets initial status to `Created`.
- Returns a strictly incrementing ID (1, 2, 3, ...).

**Panics:**
- If `milestone_amounts` is empty.

**Example:**
```
Client calls create_contract(client_addr, freelancer_addr, [200, 400, 600])
-> Returns contract_id = 1
```

---

### `deposit_funds`

**Signature:**
```rust
pub fn deposit_funds(
    env: Env,
    contract_id: u32,
    amount: i128,
) -> bool
```

**Description:**
Client deposits funds to activate the contract. Only the client may call this.

**Behavior:**
- Requires caller to be the contract's `client`.
- Requires contract to be in `Created` status.
- Transitions status to `Funded`.
- Returns `true` on success.

**Panics:**
- If contract does not exist.
- If contract is not in `Created` status (prevents re-funding).
- If `amount <= 0`.
- If caller is not the client (auth check).

**Example:**
```
client.deposit_funds(contract_id, 1_200_000_000)  // 12 XLM
-> true; contract now Funded
```

---

### `release_milestone`

**Signature:**
```rust
pub fn release_milestone(
    env: Env,
    contract_id: u32,
    milestone_id: u32,
) -> bool
```

**Description:**
Client releases a single milestone payment to the freelancer. Only the client may call this.

**Behavior:**
- Requires caller to be the contract's `client`.
- Requires contract to be in `Funded` status.
- Sets the milestone's `released` flag to `true`.
- Returning `true` on success.

**Panics:**
- If contract does not exist.
- If contract is not in `Funded` status.
- If `milestone_id` is out of range.
- If milestone has already been released (prevents double-release).
- If caller is not the client (auth check).

**Example:**
```
client.release_milestone(contract_id, 0)  // Release 1st milestone
-> true
client.release_milestone(contract_id, 1)  // Release 2nd milestone
-> true
```

---

### `complete_contract`

**Signature:**
```rust
pub fn complete_contract(
    env: Env,
    contract_id: u32,
) -> bool
```

**Description:**
Client marks the contract as `Completed`, enabling reputation issuance.

**Behavior:**
- Requires caller to be the contract's `client`.
- Requires contract to be in `Funded` status.
- **Enforces final-settlement gate:** all milestones must have `released = true`.
- Transitions status to `Completed`.
- Returns `true` on success.

**Panics:**
- If contract does not exist.
- If contract is not in `Funded` status.
- If any milestone has `released = false`.
- If caller is not the client (auth check).

**Critical Role:**
This function is the gatekeeper for reputation issuance. By requiring all milestones to be released _before_ allowing the `Completed` transition, it ensures the final-settlement constraint is established in the contract state itself, not just verified at issuance time.

**Example:**
```
client.release_milestone(contract_id, 0)
client.release_milestone(contract_id, 1)
client.release_milestone(contract_id, 2)
client.complete_contract(contract_id)  // All must be released
-> true; contract now Completed and ready for reputation
```

---

### `issue_reputation`

**Signature:**
```rust
pub fn issue_reputation(
    env: Env,
    contract_id: u32,
    rating: u32,
) -> bool
```

**Description:**
Issues a one-time reputation credential for the freelancer after contract completion and final settlement.

**Reputation Issuance Constraints** (enforced in order):

1. **Contract existence** - The contract identified by `contract_id` must exist in persistent storage. Panics with `"contract not found"` if missing.

2. **Completion gate** - The contract's `status` must be `Completed`. Reputation cannot be issued for contracts in `Created`, `Funded`, or `Disputed` states. Panics with `"reputation can only be issued after contract completion"`.

3. **Final settlement** - Every milestone must have `released == true`. This ensures no outstanding payment obligations remain. Panics with `"reputation can only be issued after final settlement of all milestones"`.

4. **Single issuance** - A credential can be issued **at most once per contract**. An immutable `ReputationIssued(contract_id)` flag is set in persistent storage before the event is emitted, preventing replay attacks and accidental double-issuance. Panics with `"reputation already issued for this contract"`.

5. **Valid rating** - The `rating` value must be in the inclusive range `[1, 5]`. Panics with `"rating must be between 1 and 5"`.

**Effects on Success:**
- Sets `DataKey::ReputationIssued(contract_id)` to `true` in persistent storage.
- Emits an observable `reputation_issued` event with the contract ID, freelancer address, and rating.
- Returns `true`.

**Event Schema:**
```
Topic: ("reputation_issued")
Data:  (contract_id: u32, freelancer: Address, rating: u32)
```

**Panics:**
- See constraint list above.

**Example:**
```
// After successful completion and final settlement:
client.issue_reputation(contract_id, 5)
-> true; event emitted for indexers
// Event: ("reputation_issued", contract_id=1, freelancer=0xABC..., rating=5)
```

---

### `cancel_contract`

**Signature:**
```rust
pub fn cancel_contract(
    env: Env,
    contract_id: u32,
    caller: Address,
) -> bool
```

**Description:**
Safely cancels an escrow contract under agreed conditions, enabling refunds and dispute resolution.

**Cancellation Policy by Status:**

#### 1. **Created Status (No Funds Deposited)**
   - **Who can cancel:** Either the client or freelancer (unilateral).
   - **Reasoning:** No funds at risk; both parties can freely exit.
   - **Effect:** Contract moves to `Cancelled` state. No refund needed.
   - **Reason recorded:** `ClientInitiated` or `FreelancerInitiated`.

#### 2. **Funded Status (Funds In Escrow)**
   Multiple paths for safe cancellation:

   **Option A: Client Cancels (Unilateral)**
   - **Allowance:** Client can cancel if and only if **no milestones have been released**.
   - **Reasoning:** Prevents freelancer from receiving partial payment then forcing cancellation.
   - **Effect:** Funds are refunded to client. Contract moves to `Cancelled`.
   - **Reason recorded:** `ClientInitiated`.

   **Option B: Freelancer Initiates (Mutual Agreement)**
   - **Allowance:** Freelancer can call `cancel_contract` at any time during `Funded`.
   - **Reasoning:** Allows freelancer to exit (e.g., if unable to complete work) without requiring client approval after acceptance.
   - **Effect:** Contract moves to `Cancelled`. Funds are returned to client.
   - **Reason recorded:** `MutualAgreement`.

   **Option C: Arbiter Approves**
   - **Allowance:** If an arbiter exists, arbiter can cancel at any time during `Funded`.
   - **Reasoning:** Useful for dispute resolution or third-party intervention.
   - **Effect:** Contract moves to `Cancelled`. Funds are returned to client.
   - **Reason recorded:** `ArbiterApproved`.

#### 3. **Disputed Status**
   - **Who can cancel:** Arbiter only (if defined).
   - **Reasoning:** Dispute resolution requires neutral third party.
   - **Effect:** Contract moves to `Cancelled`. Funds are returned to client.
   - **Reason recorded:** `ArbiterApproved`.

#### 4. **Completed Status**
   - **Who can cancel:** No one.
   - **Reasoning:** Contract fully executed; cancellation would violate final settlement.
   - **Effect:** Panics with `"Cannot cancel a completed contract"`.

#### 5. **Already Cancelled**
   - **Who can cancel:** No one.
   - **Reasoning:** Double-cancellation would corrupt audit trail.
   - **Effect:** Panics with `"Contract already cancelled"`.

**Constraints:**

- **Atomicity:** Status change, reason, timestamp, and handler address are recorded together in a single transaction.
- **Immutability:** Once cancelled, a contract cannot transition back to `Created` or `Funded`.
- **Event Emission:** On success, emits `contract_cancelled` event for audit trail.
- **Authorization:** Governed by strict policy per status (see above).

**Behavior:**
- Requires caller authentication (`require_auth()`).
- Transitions contract status to `Cancelled`.
- Records `cancellation_reason`, `cancelled_at`, and `cancelled_by`.
- Emits a `contract_cancelled` event for off-chain tracking.
- Returns `true` on success.

**Panics:**
- If contract does not exist: `"Contract not found"`.
- If contract is already cancelled: `"Contract already cancelled"`.
- If contract is completed: `"Cannot cancel a completed contract"`.
- If in `Created` state and caller is neither client nor freelancer: `"Caller must be client or freelancer to cancel in Created state"`.
- If in `Funded` state, client tries to cancel after any release: `"Client cannot cancel after milestones have been released"`.

**Examples:**

```
// Example 1: Cancel in Created state (freelancer exits early)
contract = create_contract(client, freelancer, [1000])
cancel_contract(1, freelancer)  // Freelancer calls
-> true; reason = FreelancerInitiated

// Example 2: Cancel in Funded state (client refunds before work)
contract = create_contract(client, freelancer, [1000])
deposit_funds(1, 1000)
cancel_contract(1, client)  // Client calls before any release
-> true; reason = ClientInitiated

// Example 3: Arbiter cancels due to dispute
contract = create_contract(client, freelancer, [1000], arbiter=arbiter)
deposit_funds(1, 1000)
cancel_contract(1, arbiter)  // Arbiter intervenes
-> true; reason = ArbiterApproved
```

---

### `get_contract`

**Signature:**
```rust
pub fn get_contract(
    env: Env,
    contract_id: u32,
) -> EscrowContract
```

**Description:**
Retrieves the full contract data including status, cancellation details, and milestones.

**Behavior:**
- Returns the complete `EscrowContract` structure.
- Useful for state queries and verification.

**Panics:**
- If contract does not exist: `"Contract not found"`.

**Example:**
```
contract = get_contract(1)
if contract.status == Cancelled {
    println!("Cancelled by {:?}", contract.cancelled_by);
}
```

---

### `hello`

**Signature:**
```rust
pub fn hello(_env: Env, to: Symbol) -> Symbol
```

**Description:**
Echo function for smoke-testing connectivity and CI health checks.

**Behavior:**
- Returns the input `to` unchanged.

**Example:**
```
hello(symbol_short!("World")) -> Symbol("World")
```

---

## Lifecycle Summary

```
Main Flow:
1. Created    -> Funded        (client deposits)
2. Funded     -> Released      (client releases milestones)
3. Released   -> Completed     (all milestones released)
4. Completed  -> Reputation    (freelancer reputation issued)

Alternative Flows:
- Created     -> Cancelled (either party exits early, no funds)
- Funded      -> Cancelled (refund scenarios, client/arbiter/freelancer)
- Disputed    -> Cancelled (arbiter resolution)

Key Transitions:
Created -> Funded -> Completed
   |        |           |
   +------> Cancelled <--+-- Disputed
```

---

## Authorization Model

### Access Control Summary

| Function | Caller | Auth Required | Notes |
|----------|--------|---------------|-------|
| `create_contract` | Anyone | No | Creates contract |
| `deposit_funds` | Client only | Yes | Moves to Funded |
| `approve_milestone_release` | Varies | Yes | By authorization scheme |
| `release_milestone` | Varies | Yes | By authorization scheme |
| `cancel_contract` | Varies | Yes | By contract status and policy (see below) |
| `get_contract` | Anyone | No | Read-only query |
| `issue_reputation` | Anyone | No* | Gated by contract state |
| `hello` | Anyone | No | Echo/test only |

**\* `issue_reputation` notes:** No explicit auth check, but gated by:
- Contract must exist
- Contract must be `Completed`
- All milestones must be released
- Reputation flag must not be set

### Cancellation Authorization Policy

| Contract Status | Cancelled By | Condition | Reason Code |
|-----------------|--------------|-----------|-------------|
| **Created** | Client | Always | `ClientInitiated` |
| **Created** | Freelancer | Always | `FreelancerInitiated` |
| **Funded** | Client | No releases | `ClientInitiated` |
| **Funded** | Freelancer | Always | `MutualAgreement` |
| **Funded** | Arbiter | If defined | `ArbiterApproved` |
| **Disputed** | Arbiter | If defined | `ArbiterApproved` |
| **Completed** | ❌ No one | Forbidden | N/A |
| **Cancelled** | ❌ No one | Forbidden (double-cancel) | N/A |

---

## Data Structures - Full Reference

### `ContractStatus` Enum
```rust
pub enum ContractStatus {
    Created = 0,    // Awaiting deposit
    Funded = 1,     // Funds deposited; work in progress
    Completed = 2,  // All milestones released; final
    Disputed = 3,   // Dispute raised; payments paused
    Cancelled = 4,  // Contract cancelled; funds returned
}
```

### `CancellationReason` Enum
```rust
pub enum CancellationReason {
    MutualAgreement = 0,    // Both parties agreed
    ClientInitiated = 1,    // Client initiated (no releases)
    FreelancerInitiated = 2, // Freelancer initiated
    ArbiterApproved = 3,    // Arbiter approved
    TimeoutExpired = 4,     // Timeout (future)
}
```

### `EscrowContract` Struct (Extended)
```rust
pub struct EscrowContract {
    pub client: Address,                      // Client party
    pub freelancer: Address,                  // Freelancer party
    pub arbiter: Option<Address>,             // Optional dispute resolver
    pub milestones: Vec<Milestone>,           // Deliverables & payments
    pub status: ContractStatus,               // Current status
    pub release_auth: ReleaseAuthorization,  // Release authorization scheme
    pub created_at: u64,                      // Creation timestamp
    pub cancellation_reason: Option<CancellationReason>,  // Why cancelled (if applicable)
    pub cancelled_at: Option<u64>,            // When cancelled (if applicable)
    pub cancelled_by: Option<Address>,        // Who cancelled (if applicable)
}
```

---

## Access Control Summary (Previous Section)

| Function | Caller | Auth Required |
|----------|--------|---------------|
| `create_contract` | Anyone | No |
| `deposit_funds` | Client only | Yes (`client.require_auth()`) |
| `release_milestone` | Client only | Yes (`client.require_auth()`) |
| `complete_contract` | Client only | Yes (`client.require_auth()`) |
| `cancel_contract` | Varies | Yes (strict policy) |
| `issue_reputation` | Anyone | No* |
| `hello` | Anyone | No |

* `issue_reputation` has no explicit authorization check but is gated by contract state (must be `Completed`) and single-issuance flag. In practice, only the freelancer or their designate would call this after work is complete.

---

## Security Considerations

### Threat Model & Mitigations

#### 1. **Premature Reputation Issuance**

**Threat:** Freelancer issues reputation before delivering all work.

**Mitigation:**
- Reputation can only be issued when contract is in `Completed` status.
- `Completed` status requires all milestones to be released by the client.
- Client controls when to release each milestone (milestone-by-milestone approval).

#### 2. **Double-Issuance / Replay Attacks**

**Threat:** Same reputation event issued twice or replayed on-chain.

**Mitigation:**
- `ReputationIssued(contract_id)` flag is immutably set to `true` before event emission.
- Second call to `issue_reputation` for the same `contract_id` panics immediately.
- Flag is **never cleared**, preventing accidental re-issuance even if contract state changes.

#### 3. **Milestone Released Twice**

**Threat:** Client accidentally or maliciously releases the same milestone payment twice.

**Mitigation:**
- Each milestone track `released: bool` flag.
- `release_milestone` checks `assert!(!milestone.released, ...)` and panics if true.
- After setting to `true`, the flag is immutable for that milestone.

#### 4. **Unauthorized Fund Release**

**Threat:** Non-client releases funds from escrow.

**Mitigation:**
- `deposit_funds()` and `release_milestone()` require explicit `client.require_auth()` check.
- Soroban SDK's auth mechanism verifies the signer's cryptographic credentials.
- Only the client's credentials can pass the auth check.

#### 5. **Contract Mutation During Reputation Issuance**

**Threat:** Contract state changes between checks in `issue_reputation`.

**Mitigation:**
- Soroban transactions are atomic. State reads and writes execute as a single unit.
- Contract is loaded once at the start of the function and checked at multiple points.
- The immutable `ReputationIssued` flag ensures idempotency even if called twice (second call panics cleanly).

#### 6. **Insufficient Funds**

**Threat:** Client issues a contract for total milestone amount greater than deposited.

**Mitigation (out-of-scope):**
- The current contract **does not track deposited amounts** — it assumes integration with an asset contract (e.g., USDC on Stellar).
- In a production system, asset transfers would be atomic with escrow state updates (e.g., via `InvokeContractOp` with multiple contract calls).
- For this specification, the assumption is that off-chain coordination or a higher-level orchestration layer ensures client has sufficient balance.

### Panic Messages (Security by Clarity)

All panics include descriptive messages to aid auditors and developers in identifying constraint violations:

- `"at least one milestone required"` – Contract creation guard
- `"contract not found"` – Contract lookup failure
- `"contract not in Created status"` – Deposit precondition
- `"contract not in Funded status"` – Release/complete precondition
- `"deposit amount must be positive"` – Input validation
- `"milestone_id out of range"` – Index validation
- `"milestone already released"` – Double-release guard
- `"all milestones must be released before completing"` – Final settlement gate
- `"reputation can only be issued after contract completion"` – Completion gate
- `"reputation can only be issued after final settlement of all milestones"` – Settlement gate
- `"reputation already issued for this contract"` – Single-issuance guard
- `"rating must be between 1 and 5"` – Rating range validation

---

## Test Coverage

See [tests.md](tests.md) for comprehensive test suite documentation, including:
- Happy path scenarios
- Each constraint violation
- Edge cases (empty milestones, out-of-range indices)
- Multi-contract isolation
- Idempotency and replay prevention

---

## Deployment Considerations

1. **Network:** Soroban (currently testnet/public Stellar network choice)
2. **Asset Integration:** Contracts may need to invoke an asset contract (e.g., Stellar's native asset or USDC) for actual fund transfers. This spec assumes those details are handled off-contract.
3. **Indexing:** Off-chain indexers can track `reputation_issued` events to build freelancer reputation scores.
4. **Dispute Handling:** The `Disputed` status is defined in the `ContractStatus` enum but not used in the current implementation. Future versions may add dispute resolution logic.

---

## Version

- **Version:** 0.1.0
- **Last Updated:** 2026-03-24
- **Authors:** TalentTrust
- **License:** MIT
