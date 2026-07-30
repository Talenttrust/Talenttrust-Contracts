# Requirements Document

## Introduction

The milestones entrypoints in the TalentTrust escrow smart contract
(`contracts/escrow/src/milestones.rs`) currently accept arguments without explicit
numeric or length bounds, risking bad on-chain state when callers supply
out-of-range values. This feature adds structured bounds validation — backed by
typed `EscrowError` codes — to every milestones entrypoint that accepts a
user-supplied numeric or string argument.

Scope is limited to the milestones module (`milestones.rs`,
`milestones_consts.rs`) and the constants/types it depends on. All existing
accepted inputs must continue to be accepted; only out-of-range values are newly
rejected.

---

## Glossary

- **Milestones Entrypoints**: The public contract functions that operate on milestone
  data: `release_milestone`, `refund_unreleased_milestones`, `submit_work_evidence`,
  `get_milestone`, `get_milestones`, `get_milestone_approvals`,
  `get_approval_deadline`, `get_work_evidence`, and `is_milestone_overdue`.
- **Milestone_Index**: A zero-based `u32` index into the milestone vector for a
  given escrow contract. Valid range: `[0, milestones.len() − 1]`.
- **Work_Evidence**: A Soroban `String` submitted by the freelancer to document
  completed work. Length is measured in UTF-8 bytes via `String::len()`.
- **Milestone_Indices_Vec**: A `Vec<u32>` of milestone indices supplied to
  `refund_unreleased_milestones`. Must be non-empty, free of duplicates, and every
  element must be a valid `Milestone_Index`.
- **EscrowError**: The `#[contracterror]` enum defined in `lib.rs`; all typed
  error codes for the escrow contract live here.
- **Validator**: The bounds-validation logic inside the milestones entrypoints
  (not a separate contract or module — validation runs in-line before state reads
  or writes).
- **MAX_WORK_EVIDENCE_BYTES**: The maximum byte length allowed for a work-evidence
  string. Currently **1 000** bytes (matching the existing guard in
  `submit_work_evidence_impl`), centralised as a named constant in
  `milestones_consts.rs`.
- **MIN_WORK_EVIDENCE_BYTES**: The minimum byte length for a work-evidence string.
  **1** byte — empty evidence is meaningless.
- **WORK_EVIDENCE_TOO_LONG**: `EscrowError::EvidenceTooLong` — returned when
  `evidence.len() > MAX_WORK_EVIDENCE_BYTES`.
- **WORK_EVIDENCE_EMPTY**: `Error::EvidenceTooLong` used for the too-long case;
  the existing `Error::EvidenceTooLong` variant covers the over-limit path. For
  empty evidence a distinct error (`Error::EmptyEvidence`) is introduced.

---

## Requirements

### Requirement 1: Milestone Index Bounds for `release_milestone`

**User Story:** As a contract client or arbiter, I want `release_milestone` to
reject an out-of-range milestone index with a typed error, so that callers
receive actionable feedback and the contract never panics on an invalid index.

#### Acceptance Criteria

1. WHEN `milestone_index` is greater than or equal to `milestones.len()` for the
   specified contract, THE Validator SHALL reject the call with
   `Error::IndexOutOfBounds` before performing any auth or state mutation.
2. WHEN `milestone_index` is `u32::MAX` and the contract has fewer than
   `u32::MAX + 1` milestones, THE Validator SHALL reject the call with
   `Error::IndexOutOfBounds`.
3. WHEN `milestone_index` is exactly `milestones.len() − 1` (the last valid
   index) and all other preconditions are met, THE Validator SHALL accept the
   call and proceed with the release flow.
4. IF the contract identified by `contract_id` does not exist, THEN THE Validator
   SHALL reject the call with `EscrowError::ContractNotFound` before performing
   any index check.

---

### Requirement 2: Milestone Index Bounds for `refund_unreleased_milestones`

**User Story:** As a contract client, I want `refund_unreleased_milestones` to
validate every supplied milestone index against the actual milestone count, so
that partial-index vectors cannot corrupt accounting state.

#### Acceptance Criteria

1. WHEN `milestone_indices` is empty, THE Validator SHALL reject the call with
   `EscrowError::EmptyRefundRequest` before loading any contract state.
2. WHEN `milestone_indices` contains duplicate values, THE Validator SHALL
   unconditionally reject the call with `EscrowError::DuplicateMilestoneInRefund`,
   regardless of whether the indices are otherwise valid.
3. WHEN any element of `milestone_indices` is greater than or equal to
   `milestones.len()`, THE Validator SHALL reject the call with
   `Error::IndexOutOfBounds`.
4. WHEN `milestone_indices` contains `u32::MAX` and the milestone vector has
   fewer entries than `u32::MAX + 1`, THE Validator SHALL reject the call with
   `Error::IndexOutOfBounds`.
5. WHEN every element of `milestone_indices` is a valid, non-duplicate index into
   an unreleased, non-refunded milestone, THE Validator SHALL accept the call and
   proceed with the refund flow.

---

### Requirement 3: Milestone Index Bounds for `submit_work_evidence`

**User Story:** As a freelancer, I want `submit_work_evidence` to reject an
out-of-range index with a typed error, so that the entrypoint fails safely
without state corruption.

#### Acceptance Criteria

1. WHEN `milestone_index` is greater than or equal to `milestones.len()` for the
   specified contract, THE Validator SHALL reject the call with
   `Error::IndexOutOfBounds`.
2. WHEN `milestone_index` is exactly `milestones.len() − 1` and all other
   preconditions are met, THE Validator SHALL accept the call.

---

### Requirement 4: Work Evidence Length Bounds for `submit_work_evidence`

**User Story:** As a freelancer, I want `submit_work_evidence` to reject evidence
strings that are empty or exceed the protocol maximum, so that on-chain storage
is bounded and callers receive explicit typed feedback.

#### Acceptance Criteria

1. WHEN `evidence.len()` is `0` (empty string), THE Validator SHALL reject the
   call with `Error::EmptyEvidence`.
2. WHEN `evidence.len()` is greater than `MAX_WORK_EVIDENCE_BYTES` (1 000),
   THE Validator SHALL reject the call with `Error::EvidenceTooLong`.
3. WHEN `evidence.len()` is exactly `MAX_WORK_EVIDENCE_BYTES`, THE Validator
   SHALL accept the call and store the evidence.
4. WHEN `evidence.len()` is exactly `1` (minimum), THE Validator SHALL accept
   the call.
5. THE Milestones_Module SHALL expose `MAX_WORK_EVIDENCE_BYTES` and
   `MIN_WORK_EVIDENCE_BYTES` as named `pub const` values in
   `milestones_consts.rs`, so that test and governance code can reference limits
   symbolically rather than by literal.

---

### Requirement 5: Named Constants in `milestones_consts.rs`

**User Story:** As a developer reviewing or testing the milestones module, I want
all protocol-level bounds to be defined as named constants in `milestones_consts.rs`,
so that limits are documented in one place and test assertions never depend on
literals.

#### Acceptance Criteria

1. THE Milestones_Module SHALL define `MAX_WORK_EVIDENCE_BYTES: u32 = 1_000` in
   `milestones_consts.rs`.
2. THE Milestones_Module SHALL define `MIN_WORK_EVIDENCE_BYTES: u32 = 1` in
   `milestones_consts.rs`.
3. FOR ALL uses of the evidence length bound in `milestones.rs`, the source SHALL
   reference `MAX_WORK_EVIDENCE_BYTES` and `MIN_WORK_EVIDENCE_BYTES` rather than
   inline literals.
4. WHEN the constants in `milestones_consts.rs` are changed, THE Milestones_Module
   SHALL enforce the updated bounds in all entrypoints without requiring changes
   to call sites beyond the constant definition.

---

### Requirement 6: New `Error` Variant for Empty Evidence

**User Story:** As an API consumer, I want a distinct typed error when I submit
an empty work-evidence string, so that I can distinguish "too long" from "empty"
without inspecting string content.

#### Acceptance Criteria

1. THE EscrowContract SHALL expose a new `Error::EmptyEvidence` variant in the
   `Error` contracterror enum.
2. WHEN `submit_work_evidence` is called with an empty string, THE Validator SHALL
   return `Error::EmptyEvidence`.
3. WHEN `submit_work_evidence` is called with a non-empty string that exceeds
   `MAX_WORK_EVIDENCE_BYTES`, THE Validator SHALL return `Error::EvidenceTooLong`
   (not `EmptyEvidence`).

---

### Requirement 7: Preservation of All Existing Accepted Inputs

**User Story:** As an integrator with contracts already on-chain, I want all
currently-accepted milestone entrypoint inputs to remain accepted after this
change, so that the deployment is backward-compatible.

#### Acceptance Criteria

1. THE Validator SHALL accept any `milestone_index` value in the range
   `[0, milestones.len() − 1]` that was previously accepted before this feature.
2. THE Validator SHALL accept any `evidence` string with byte length in the range
   `[1, MAX_WORK_EVIDENCE_BYTES]` that was previously accepted.
3. THE Validator SHALL accept any `milestone_indices` vector that was previously
   accepted by `refund_unreleased_milestones`.
4. FOR ALL valid inputs, the Validator SHALL produce identical on-chain state
   changes as the pre-validation code path (validation is purely additive — no
   business logic changes).

---

### Requirement 8: Test Coverage for Boundary Values

**User Story:** As a code reviewer, I want comprehensive tests covering min, max,
zero, and over-limit values for every new validation guard, so that regressions
are caught before deployment.

#### Acceptance Criteria

1. THE Test_Suite SHALL include at least one test for each of the following
   boundary classes for every numeric/length bound added:
   - Exact minimum (accepted)
   - Exact maximum (accepted)
   - Zero / below minimum (rejected with correct error)
   - One above maximum (rejected with correct error)
2. WHERE the system contains one or more milestones entrypoints, THE Test_Suite
   SHALL include at least one regression test per entrypoint confirming that a
   previously-valid input still succeeds after this change.
3. WHEN tests for `release_milestone` index bounds run, THE Test_Suite SHALL
   cover `milestone_index = 0`, `milestone_index = milestones.len() − 1`, and
   `milestone_index = milestones.len()` (out of bounds by 1).
4. WHEN tests for `submit_work_evidence` length bounds run, THE Test_Suite SHALL
   cover evidence of length `0`, `1`, `MAX_WORK_EVIDENCE_BYTES`, and
   `MAX_WORK_EVIDENCE_BYTES + 1`.
5. WHEN tests for `refund_unreleased_milestones` index bounds run, THE Test_Suite
   SHALL cover an empty indices vector, a duplicate-index vector, an
   out-of-bounds single index, and a valid single index.
6. THE Test_Suite SHALL be placed in a new test file
   `contracts/escrow/src/test/milestones_bounds_validation.rs` and registered in
   `contracts/escrow/src/test/mod.rs`.
