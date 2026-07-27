# Escrow Error Catalog (Ordered)

**Contract**: `contracts/escrow` (Soroban)  
**Enum source of truth**: `contracts/escrow/src/types.rs` → `pub enum Error`  
**Acceptance (this repo snapshot)**: Catalog matches numbering **1..=23** exactly.

This document is a single reference mapping each error **code** to:
- The **variant name**
- The **entrypoint(s)** that raise it (implemented and/or intended)
- The **precise trigger condition**
- Whether it is **live** or **reserved / not currently reachable**

> Notes
> - “Live” means the error is raised by the currently implemented public entrypoints in `contracts/escrow/src/lib.rs` (based on code inspection).
> - “Reserved” means it exists in the enum but is not provably reachable from current public entrypoints (or is part of planned work).

---

## Legend

- **Status**:
  - ✅ **Live**: reachable from current contract code paths
  - 🟡 **Partially live / ambiguous**: referenced in code comments/docs or by helper modules, but not clearly enforced in all paths
  - 🔴 **Reserved / unused**: not currently reachable (planned)

- **Entrypoints** refer to public `#[contractimpl]` fns in `contracts/escrow/src/lib.rs`, such as:
  - `create_contract`
  - `deposit_funds`
  - `approve_milestone_release`
  - `release_milestone`
  - `refund_unreleased_milestones`
  - `get_contract`
  - `get_milestones`
  - `get_refundable_balance`
  - `get_milestone_approvals`
  - Plus admin/pause/emergency entrypoints **if present** in your `lib.rs` (your SECURITY.md already references them)

---

## Code 1: `AlreadyInitialized` ✅ Live

**Entrypoint(s)**:
- `initialize` (admin setup)

**Trigger condition**:
- Initialization was already performed and a second initialization attempt is made.

**Precise condition (conceptual)**:
- Stored initialization flag indicates the contract is already initialized.

**Why it exists**:
- Enforces single-use initialization / prevents admin reset.

---

## Code 2: `NotInitialized` ✅ Live

**Entrypoint(s)**:
- Most public entrypoints that require initialization (create/deposit/release/refund/etc.)

**Trigger condition**:
- Any operation requiring initialization is invoked before `initialize`.

**Precise condition (conceptual)**:
- Initialization flag not set.

**Security impact**:
- Ensures admin/pause/emergency gates exist before lifecycle operations.

---

## Code 3: `IndexOutOfBounds` ✅ Live

**Entrypoint(s)**:
- `release_milestone`
- `refund_unreleased_milestones`

**Trigger condition**:
- A milestone index is >= `milestones.len()`.

**Precise condition (as implemented)**:
- In `release_milestone`:
  - `if milestone_index >= milestones.len() { panic_with_error(IndexOutOfBounds) }`
- In `refund_unreleased_milestones`:
  - For any refund index `idx`:
    - `if idx >= milestones.len() { panic_with_error(IndexOutOfBounds) }`

---

## Code 4: `AlreadyReleased` ✅ Live

**Entrypoint(s)**:
- `release_milestone`
- `refund_unreleased_milestones`

**Trigger condition**:
- Attempt to release a milestone that has already been released, or refund a released milestone.

**Precise condition (as implemented)**:
- `if milestone.released { panic_with_error(AlreadyReleased) }`

---

## Code 5: `InvalidStatusTransition` 🟡 Partially live / ambiguous

**Entrypoint(s)**:
- Lifecycle entrypoints that gate by status (exact set depends on your `lib.rs`)

**Trigger condition**:
- The operation is incompatible with current `ContractStatus`.

**Precise condition (conceptual)**:
- Example: trying to deposit after Funded/Completed, release before Funded, etc.

**Note**:
- In the `lib.rs` snapshot fetched earlier, some paths use `InvalidState` for status gating; this code still exists for more specific transition errors or older paths. Keep mapped in integrators even if not currently thrown.

---

## Code 6: `EmptyRefundRequest` ✅ Live

**Entrypoint(s)**:
- `refund_unreleased_milestones`

**Trigger condition**:
- Refund called with an empty list of milestone indices.

**Precise condition (as implemented)**:
- `if milestone_indices.is_empty() { panic_with_error(EmptyRefundRequest) }`

---

## Code 7: `DuplicateMilestoneInRefund` ✅ Live

**Entrypoint(s)**:
- `refund_unreleased_milestones`

**Trigger condition**:
- Refund request includes the same milestone index more than once.

**Precise condition (as implemented)**:
- Nested loop checks duplicates; on match:
  - `panic_with_error(DuplicateMilestoneInRefund)`

---

## Code 8: `AlreadyRefunded` ✅ Live

**Entrypoint(s)**:
- `release_milestone`
- `refund_unreleased_milestones`

**Trigger condition**:
- Attempt to refund a milestone that is already refunded, or release a refunded milestone.

**Precise condition (as implemented)**:
- `if milestone.refunded { panic_with_error(AlreadyRefunded) }`

---

## Code 9: `InsufficientFunds` ✅ Live

**Entrypoint(s)**:
- `release_milestone`
- `refund_unreleased_milestones`

**Trigger condition**:
- The available balance is less than the amount required for the operation.

**Precise condition (as implemented)**:
- Compute:
  - `available_balance = contract.funded_amount - contract.released_amount - contract.refunded_amount`
- Release:
  - `if available_balance < milestone.amount { panic_with_error(InsufficientFunds) }`
- Refund:
  - `if available_balance < total_refund_amount { panic_with_error(InsufficientFunds) }`

**Security note**:
- This is the core accounting invariant gate that prevents over-allocation.

---

## Code 10: `ContractNotFound` ✅ Live

**Entrypoint(s)**:
- Any entrypoint that loads `DataKey::Contract(contract_id)`:
  - `deposit_funds`
  - `release_milestone`
  - `refund_unreleased_milestones`
  - `get_contract`
  - `get_milestones`
  - `get_refundable_balance`
  - (and others, if present)

**Trigger condition**:
- No contract exists at that ID in storage.

**Precise condition (as implemented)**:
- `get(...).unwrap_or_else(|| panic_with_error(ContractNotFound))`

---

## Code 11: `UnauthorizedRole` ✅ Live

**Entrypoint(s)**:
- `deposit_funds`
- `release_milestone` (role gating depends on `ReleaseAuthorization`)
- `approve_milestone_release` (via approvals module)
- Any other role-gated lifecycle/admin functions (pause/emergency, etc.)

**Trigger condition**:
- Caller is not authorized for the operation (not client/freelancer/arbiter/admin as required).

**Precise condition (examples)**:
- Deposit:
  - `if caller != contract.client { panic_with_error(UnauthorizedRole) }`
- Release:
  - Based on `release_authorization`, require client and/or arbiter membership.

---

## Code 12: `MissingArbiter` ✅ Live

**Entrypoint(s)**:
- `create_contract`

**Trigger condition**:
- `ReleaseAuthorization` requires an arbiter but no arbiter address provided.

**Precise condition (as implemented)**:
- If `release_authorization` is `ArbiterOnly` or `ClientAndArbiter`:
  - `if arbiter.is_none() { panic_with_error(MissingArbiter) }`

---

## Code 13: `InvalidArbiter` ✅ Live

**Entrypoint(s)**:
- `create_contract`

**Trigger condition**:
- Arbiter equals the client or freelancer address.

**Precise condition (as implemented)**:
- If `arbiter == client || arbiter == freelancer`:
  - `panic_with_error(InvalidArbiter)`

---

## Code 14: `InvalidParticipants` ✅ Live

**Entrypoint(s)**:
- `create_contract`

**Trigger condition**:
- Client and freelancer addresses are identical.

**Precise condition (as implemented)**:
- `if client == freelancer { panic_with_error(InvalidParticipants) }`

---

## Code 15: `AmountMustBePositive` ✅ Live

**Entrypoint(s)**:
- `deposit_funds`
- (Often also used by `create_contract` / milestone validation in many designs; depends on your current `lib.rs`)

**Trigger condition**:
- Any amount is `<= 0`.

**Precise condition (as implemented, deposit)**:
- `if amount <= 0 { panic_with_error(AmountMustBePositive) }`

---

## Code 16: `InvalidState` ✅ Live

**Entrypoint(s)**:
- `deposit_funds`
- `release_milestone`
- Other lifecycle functions that enforce paused/emergency/status gating

**Trigger condition**:
- Contract is in a state that forbids the operation (including pause/emergency controls).

**Precise condition (examples)**:
- Deposit only allowed from `ContractStatus::Created`
- Release only allowed from `ContractStatus::Funded`
- Mutations forbidden when paused or emergency controls active

---

## Code 17: `MilestoneAlreadyReleased` ✅ Live (semantic alias of Code 4)

**Entrypoint(s)**:
- `release_milestone` (and/or approval flows)

**Trigger condition**:
- Milestone already released.

**Precise condition**:
- Equivalent to Code 4, but used for more specific messaging.

**Note**:
- Integrators should treat Code 4 and Code 17 as the same user-level error.

---

## Code 18: `AlreadyApproved` 🟡 Partially live / depends on approvals module

**Entrypoint(s)**:
- `approve_milestone_release` (through `approvals::approve_milestone`)

**Trigger condition**:
- The same role (client/freelancer/arbiter) attempts to approve the same milestone twice.

**Precise condition (intended)**:
- If caller role flag is already `true` in `MilestoneApprovals`, reject.

**Status note**:
- This is expected to be live if `approvals.rs` persists approvals and checks duplicates.

---

## Code 19: `ApprovalExpired` 🟡 Partially live / depends on TTL behavior

**Entrypoint(s)**:
- `release_milestone` (via `approvals::check_approvals`)
- Possibly `approve_milestone_release` (if it reads existing approvals)

**Trigger condition**:
- Approval record missing due to TTL expiration / eviction.

**Precise condition (intended)**:
- `env.storage().temporary().get(...)` returns `None` when approval is required.

**Security note**:
- This enforces time-bounded approvals (fail-closed if approvals are stale).

---

## Code 20: `InsufficientApprovals` 🟡 Partially live / depends on approvals logic

**Entrypoint(s)**:
- `release_milestone` (via `approvals::check_approvals`)

**Trigger condition**:
- Not enough required approvals have been recorded for the configured `ReleaseAuthorization`.

**Precise condition (intended)**:
- For `ClientAndArbiter`: require both client and arbiter approval flags
- For `MultiSig`: require both client and freelancer approval flags
- For `ClientOnly`/`ArbiterOnly`: approvals may be skipped (depending on design)

---

## Code 21: `FreelancerMismatch` 🔴 Reserved / unused in current `lib.rs` snapshot

**Entrypoint(s)**:
- Intended: `issue_reputation` (or any reputation-related entrypoint)

**Trigger condition**:
- A provided freelancer address does not match the stored contract freelancer.

**Precise condition (intended)**:
- `if provided_freelancer != contract.freelancer { panic_with_error(FreelancerMismatch) }`

---

## Code 22: `InvalidRating` 🔴 Reserved / unused in current `lib.rs` snapshot

**Entrypoint(s)**:
- Intended: `issue_reputation`

**Trigger condition**:
- Rating outside allowed range (usually `1..=5`).

**Precise condition (intended)**:
- `if rating < 1 || rating > 5 { panic_with_error(InvalidRating) }`

---

## Code 23: `ReputationAlreadyIssued` 🔴 Reserved / unused in current `lib.rs` snapshot

**Entrypoint(s)**:
- Intended: `issue_reputation`

**Trigger condition**:
- Reputation already issued for this contract.

**Precise condition (intended)**:
- `if contract.reputation_issued { panic_with_error(ReputationAlreadyIssued) }`

---

## Cross-links

- Public entrypoint security notes and assumptions: [`SECURITY.md`](./SECURITY.md)
- Enum definitions: `contracts/escrow/src/types.rs` (`Error` is `#[repr(u32)]`)