# Escrow Security Notes (Escrow Error Catalog Companion)

This document reflects the escrow API currently implemented in
`contracts/escrow/src/lib.rs` and the error codes defined in
`contracts/escrow/src/types.rs` (`pub enum Error`, codes **1..=23**).

This file is **not** a duplicate of `ERROR_CATALOG.md`. It focuses on:
- Security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)
- Public entrypoint documentation (NatSpec/rustdoc-style) *as a guide*
- Cross-links from entrypoints → error codes

---

## Public Entrypoints (Doc Guide)

> This section is documentation only. It does **not** change code.  
> Keep it aligned with the signatures in `contracts/escrow/src/lib.rs`.

### `initialize(admin: Address) -> bool`
**Auth**: `admin.require_auth()`  
**Errors**:
- `AlreadyInitialized` (1): initialization already performed

**Security**:
- Single-use admin bootstrapping.

### `create_contract(client, freelancer, arbiter, milestones, release_authorization) -> u32`
**Auth**: `client.require_auth()`  
**Errors**:
- `InvalidParticipants` (14): `client == freelancer`
- `MissingArbiter` (12): arbiter required but `None`
- `InvalidArbiter` (13): arbiter equals client/freelancer
- `AmountMustBePositive` (15) or contract-specific milestone validation: any milestone amount `<= 0`

**Security**:
- Validates participants and configuration before any persistent write (fail-closed).

### `deposit_funds(contract_id, caller, amount) -> bool`
**Auth**: `caller.require_auth()`; must equal stored client  
**Errors**:
- `AmountMustBePositive` (15): `amount <= 0`
- `ContractNotFound` (10): contract missing
- `UnauthorizedRole` (11): caller not client
- `InvalidState` (16) / `InvalidStatusTransition` (5): wrong status (e.g. not Created) or paused

**Security**:
- Role gating + fail-closed amount checks.
- Balance invariant maintained by checks in downstream operations.

### `approve_milestone_release(contract_id, caller, milestone_index) -> bool`
**Auth**: `caller.require_auth()` (expected; enforced by approvals module patterns)  
**Errors** (via approvals module):
- `ContractNotFound` (10): contract missing
- `InvalidState` (16) / `InvalidStatusTransition` (5): not in correct lifecycle state
- `IndexOutOfBounds` (3): invalid milestone index
- `MilestoneAlreadyReleased` (17): milestone already released
- `UnauthorizedRole` (11): caller not permitted to approve
- `AlreadyApproved` (18): caller already approved

**Security**:
- Uses temporary storage approvals with TTL (see TTL section).

### `release_milestone(contract_id, caller, milestone_index) -> bool`
**Auth**: `caller.require_auth()` and role checks for `ReleaseAuthorization`  
**Errors**:
- `ContractNotFound` (10)
- `InvalidState` (16): not Funded / paused/emergency
- `UnauthorizedRole` (11): caller not allowed by authorization mode
- `IndexOutOfBounds` (3)
- `AlreadyReleased` (4) / `MilestoneAlreadyReleased` (17)
- `AlreadyRefunded` (8)
- `ApprovalExpired` (19) / `InsufficientApprovals` (20): approval gating
- `InsufficientFunds` (9)

**Security**:
- Fail-closed ordering: checks before writes.
- Clears approvals after release (prevents replay).

### `refund_unreleased_milestones(contract_id, milestone_indices) -> i128`
**Auth**: stored client must authorize  
**Errors**:
- `EmptyRefundRequest` (6)
- `DuplicateMilestoneInRefund` (7)
- `ContractNotFound` (10)
- `IndexOutOfBounds` (3)
- `AlreadyReleased` (4)
- `AlreadyRefunded` (8)
- `InsufficientFunds` (9)

**Security**:
- All indices validated before any mutation (atomic all-or-nothing refund).
- Balance invariant enforced by available-balance check.

### Read-only helpers
- `get_contract`: `ContractNotFound` (10)
- `get_milestones`: `ContractNotFound` (10)
- `get_refundable_balance`: `ContractNotFound` (10)
- `get_milestone_approvals`: returns `Option`, no error (approval may be evicted by TTL)

---

## Security Assumptions & Validation Notes

### Authorization (Auth)
- All role-sensitive entrypoints must call `require_auth()` on the correct address.
- Errors involved:
  - `UnauthorizedRole` (11)
  - `NotInitialized` (2) (if initialization gating exists across entrypoints)

### Overflow / Arithmetic Safety
- Amount totals and deltas must be computed safely.
- Errors involved:
  - `InsufficientFunds` (9)
  - `AmountMustBePositive` (15)

**Invariant** (core accounting):
```
available_balance = funded_amount - released_amount - refunded_amount
available_balance >= 0
```

### Fail-closed State Machine
- Operations must reject before state mutation if any guard fails.
- Errors involved:
  - `InvalidState` (16)
  - `InvalidStatusTransition` (5)
  - `AlreadyReleased` (4)
  - `AlreadyRefunded` (8)

### Storage TTL (Approvals)
Approvals are stored under:
- `DataKey::MilestoneApprovals(contract_id, milestone_index)`
- In **temporary storage** with TTL.

Errors involved:
- `ApprovalExpired` (19): approval missing/evicted
- `AlreadyApproved` (18): duplicate approvals
- `InsufficientApprovals` (20): not enough approvals

### Fee Accounting (Planned)
The enum includes `GovernedParameters` but public fee entrypoints may not exist yet.
If/when fees are implemented, they should introduce:
- Explicit rounding rules (floor/ceil)
- Tests for fee rounding edge cases
- Ledger-event emission for fee accrual/withdrawal

---

## Reserved / Not Currently Reachable Error Codes

Some errors may be defined but not reachable depending on the current set of entrypoints:
- `FreelancerMismatch` (21)
- `InvalidRating` (22)
- `ReputationAlreadyIssued` (23)

When these are implemented (e.g., reputation issuance), update:
- `ERROR_CATALOG.md` to mark them Live
- Add tests in `contracts/escrow/src/test/security.rs`

---

## Reviewer Checklist (Security)

1. Verify auth checks are present and correct for each role-gated entrypoint.
2. Verify pause/emergency (if implemented) blocks every mutating call.
3. Verify duplicate release/refund paths fail closed.
4. Verify approval TTL eviction results in `ApprovalExpired` and blocks release.
5. Verify accounting invariant holds after every mutation.
6. Ensure integrators do not treat reserved codes as live behavior.

---

## References

- Error enum: `contracts/escrow/src/types.rs` (`pub enum Error`, `#[repr(u32)]`)
- Full catalog: [`ERROR_CATALOG.md`](./ERROR_CATALOG.md)