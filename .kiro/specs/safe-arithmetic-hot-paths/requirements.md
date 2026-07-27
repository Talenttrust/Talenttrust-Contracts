# Requirements Document

## Introduction

The Talenttrust escrow contract (`contracts/escrow`) currently has two financial accounting hot paths — `release_milestone` and `refund_unreleased_milestones` — that perform raw, unchecked arithmetic on `i128` token amounts. An integer overflow or underflow on these paths would silently corrupt on-chain accounting state, enabling a malicious or buggy caller to drain or brick the escrow without triggering any error. This feature hardens those hot paths by replacing every raw arithmetic operator on financial values with checked/saturating equivalents, ensuring any overflow or underflow is caught and reported as a typed, non-panicking error. Infrastructure (error codes, helper functions) is already present in the codebase; no new primitives are needed.

---

## Glossary

- **Escrow_Contract**: The Soroban smart contract defined in `contracts/escrow/src/lib.rs` that holds deposited funds and orchestrates milestone-based payments.
- **Release_Path**: The `release_milestone` entrypoint in `lib.rs` — responsible for computing the available balance, computing the protocol fee, transferring net payout to the freelancer, and checking the accounting invariant post-write.
- **Refund_Path**: The `refund_unreleased_milestones` entrypoint in `lib.rs` — responsible for accumulating refund amounts across selected milestones, computing the available balance, and transferring the total back to the client.
- **available_balance**: The spendable balance for a contract, calculated as `funded_amount − released_amount − refunded_amount [− accumulated_fees]`. Must be computed with checked subtraction.
- **invariant_sum**: The post-write consistency check sum `released_amount + refunded_amount + new_accumulated_fees`. Must be computed with checked addition.
- **total_refund_amount**: The running total of milestone amounts accumulated during the refund validation loop. Must be computed with checked addition.
- **accumulated_fees**: The protocol fee balance stored under `DataKey::AccumulatedProtocolFees`. Used in both the available-balance deduction and the invariant-sum check on the Release_Path.
- **protocol_fee**: The per-milestone fee computed from `gross_amount × fee_bps / 10_000`. Used in both `accumulated_fees + protocol_fee` writes and the invariant check.
- **safe_add_amounts**: The function `safe_add_amounts(a, b) -> Option<i128>` defined in `amount_validation.rs`. Returns `None` on overflow.
- **safe_subtract_amounts**: The function `safe_subtract_amounts(a, b) -> Option<i128>` defined in `amount_validation.rs`. Returns `None` on underflow.
- **EscrowError**: The `contracterror` enum in `lib.rs` used to signal errors via `env.panic_with_error(...)`.
- **PotentialOverflow**: `EscrowError::PotentialOverflow = 28`. Signals that a checked arithmetic operation returned `None`.
- **AccountingInvariantViolated**: `EscrowError::AccountingInvariantViolated = 27`. Signals that the post-write sum of accounting fields exceeds `funded_amount`.
- **SECURITY_MD**: The file `contracts/escrow/docs/SECURITY.md` that documents the overflow policy for operators and auditors.
- **input_sanitization_amounts**: The test file `contracts/escrow/src/test/input_sanitization_amounts.rs` that contains unit tests for amount validation and boundary conditions.

---

## Requirements

### Requirement 1: Checked Available-Balance Computation in `release_milestone`

**User Story:** As a contract operator, I want the `release_milestone` available-balance calculation to use checked arithmetic, so that any overflow in the funded/released/refunded/fees accounting fields raises a typed error instead of silently wrapping.

#### Acceptance Criteria

1. WHEN `release_milestone` computes `available_balance`, THE Release_Path SHALL use `safe_subtract_amounts` for each successive subtraction (`funded_amount − released_amount`, then `− refunded_amount`, then `− accumulated_fees`), propagating `None` as `EscrowError::PotentialOverflow`.
2. IF any intermediate `safe_subtract_amounts` call returns `None` during the `available_balance` computation, THEN THE Release_Path SHALL call `env.panic_with_error(EscrowError::PotentialOverflow)` and halt execution before any token transfer occurs.
3. WHILE the contract is in `Accepted` state, THE Release_Path SHALL preserve all existing positivity, state, and authorization checks that precede the `available_balance` computation.

---

### Requirement 2: Checked Invariant-Sum Computation in `release_milestone`

**User Story:** As a contract operator, I want the post-write accounting invariant check in `release_milestone` to use checked arithmetic, so that a corrupted or unexpectedly large accumulated-fees value cannot cause a silent wrap that bypasses the invariant guard.

#### Acceptance Criteria

1. WHEN `release_milestone` computes `new_accumulated`, THE Release_Path SHALL use `safe_add_amounts(accumulated_fees, protocol_fee)`, propagating `None` as `EscrowError::PotentialOverflow`.
2. WHEN `release_milestone` computes `invariant_sum`, THE Release_Path SHALL use `safe_add_amounts` for both additive steps (`released_amount + refunded_amount` and the result `+ new_accumulated`), propagating `None` as `EscrowError::PotentialOverflow`.
3. IF any intermediate `safe_add_amounts` call returns `None` during the invariant-sum computation, THEN THE Release_Path SHALL call `env.panic_with_error(EscrowError::PotentialOverflow)` before writing any state.
4. WHEN the computed `invariant_sum` exceeds `contract.funded_amount`, THE Release_Path SHALL call `env.panic_with_error(EscrowError::AccountingInvariantViolated)`.

---

### Requirement 3: Checked AccumulatedProtocolFees Write in `release_milestone`

**User Story:** As a contract operator, I want the accumulated-fees update in `release_milestone` to use checked arithmetic, so that repeated fee accruals across many milestones cannot silently wrap the stored fee balance.

#### Acceptance Criteria

1. WHEN `release_milestone` accrues the protocol fee into `AccumulatedProtocolFees`, THE Release_Path SHALL use `safe_add_amounts(accumulated_fees, protocol_fee)`, propagating `None` as `EscrowError::PotentialOverflow`.
2. IF `safe_add_amounts` returns `None` during the accumulated-fees write, THEN THE Release_Path SHALL call `env.panic_with_error(EscrowError::PotentialOverflow)` before persisting any state update.
3. THE Release_Path SHALL reuse the single `safe_add_amounts(accumulated_fees, protocol_fee)` result for both the `AccumulatedProtocolFees` write (Requirement 3) and the invariant-sum step (Requirement 2), rather than recomputing it separately.

---

### Requirement 4: Checked Loop Accumulation in `refund_unreleased_milestones`

**User Story:** As a contract operator, I want the per-milestone accumulation loop in `refund_unreleased_milestones` to use checked arithmetic, so that a crafted set of milestone amounts near `i128::MAX` cannot silently overflow the running total.

#### Acceptance Criteria

1. WHEN `refund_unreleased_milestones` accumulates milestone amounts in the validation loop, THE Refund_Path SHALL replace the raw `total_refund_amount += milestone.amount` with `safe_add_amounts(total_refund_amount, milestone.amount)`, propagating `None` as `EscrowError::PotentialOverflow`.
2. IF `safe_add_amounts` returns `None` during loop accumulation, THEN THE Refund_Path SHALL call `env.panic_with_error(EscrowError::PotentialOverflow)` before executing any token transfer.

---

### Requirement 5: Checked Available-Balance Computation in `refund_unreleased_milestones`

**User Story:** As a contract operator, I want the `refund_unreleased_milestones` available-balance calculation to use checked arithmetic, so that any underflow in the contract accounting fields raises a typed error instead of producing a spurious positive balance.

#### Acceptance Criteria

1. WHEN `refund_unreleased_milestones` computes `available_balance`, THE Refund_Path SHALL use `safe_subtract_amounts` for each subtraction (`funded_amount − released_amount`, then `− refunded_amount`), propagating `None` as `EscrowError::PotentialOverflow`.
2. IF any intermediate `safe_subtract_amounts` call returns `None` during the `available_balance` computation, THEN THE Refund_Path SHALL call `env.panic_with_error(EscrowError::PotentialOverflow)` before any token transfer occurs.

---

### Requirement 6: AccountingInvariantViolated Post-Write Check in `refund_unreleased_milestones`

**User Story:** As a contract operator, I want `refund_unreleased_milestones` to perform the same post-write accounting invariant check already present in `release_milestone`, so that any corruption of the contract accounting fields is caught consistently across both financial hot paths.

#### Acceptance Criteria

1. WHEN `refund_unreleased_milestones` updates `contract.refunded_amount`, THE Refund_Path SHALL subsequently compute `invariant_sum` as `released_amount + refunded_amount + accumulated_fees` using `safe_add_amounts`, then compare it against `contract.funded_amount`.
2. IF the post-write `invariant_sum` exceeds `contract.funded_amount`, THEN THE Refund_Path SHALL call `env.panic_with_error(EscrowError::AccountingInvariantViolated)` before persisting any state or emitting events.
3. IF any `safe_add_amounts` call during the invariant-sum computation returns `None`, THEN THE Refund_Path SHALL call `env.panic_with_error(EscrowError::PotentialOverflow)`.
4. THE Refund_Path SHALL read `accumulated_fees` from `DataKey::AccumulatedProtocolFees` (defaulting to `0`) immediately before computing the post-write invariant, consistent with the pattern in Release_Path.

---

### Requirement 7: No New Error Codes

**User Story:** As a maintainer, I want the safe-arithmetic changes to reuse the existing error codes, so that the append-only error code contract is preserved and no existing tooling or indexer is broken.

#### Acceptance Criteria

1. THE Escrow_Contract SHALL NOT define any new variants in `EscrowError` or `Error` as part of this feature.
2. THE Escrow_Contract SHALL use only `EscrowError::PotentialOverflow = 28` for overflow/underflow conditions introduced by this feature.
3. THE Escrow_Contract SHALL use only `EscrowError::AccountingInvariantViolated = 27` for invariant violations introduced by this feature.
4. THE Escrow_Contract SHALL NOT renumber or remove any existing error code variants.

---

### Requirement 8: `i128::MAX` Boundary Tests in `input_sanitization_amounts`

**User Story:** As a developer, I want boundary tests near `i128::MAX` in `input_sanitization_amounts.rs` that assert typed error codes, so that overflow behaviour on all affected arithmetic paths is continuously verified by the test suite.

#### Acceptance Criteria

1. THE input_sanitization_amounts test module SHALL include a test that constructs contract state where `total_refund_amount` would overflow `i128::MAX` during loop accumulation and asserts that the call panics.
2. THE input_sanitization_amounts test module SHALL include a test that constructs contract state where the `available_balance` subtraction would underflow and asserts that the call panics.
3. THE input_sanitization_amounts test module SHALL include a test that constructs contract state where the `invariant_sum` addition would overflow `i128::MAX` and asserts that the call panics.
4. WHEN any of these boundary tests verify a panic, THE input_sanitization_amounts test module SHALL use `#[should_panic]` or equivalent Soroban test harness patterns consistent with the existing tests in the file.
5. FOR ALL boundary tests added, THE input_sanitization_amounts test module SHALL document which specific arithmetic expression each test targets via a code comment.

---

### Requirement 9: Overflow Policy Documentation in `SECURITY.md`

**User Story:** As an operator or auditor, I want a `SECURITY.md` file in `contracts/escrow/docs/` that documents the arithmetic overflow policy, so that the rationale and invariants are discoverable without reading source code.

#### Acceptance Criteria

1. THE SECURITY_MD SHALL document that all financial arithmetic in Release_Path and Refund_Path uses `safe_add_amounts` / `safe_subtract_amounts` from `amount_validation.rs`.
2. THE SECURITY_MD SHALL state that overflow and underflow conditions are reported via `EscrowError::PotentialOverflow = 28` and halt execution before any token transfer.
3. THE SECURITY_MD SHALL document the accounting invariant: `released_amount + refunded_amount + accumulated_fees ≤ funded_amount`, stating that violations are reported via `EscrowError::AccountingInvariantViolated = 27`.
4. THE SECURITY_MD SHALL state that `deposit_funds` is already safe via `.checked_add` in `deposit::validate_deposit` and is therefore out of scope for this feature.
5. THE SECURITY_MD SHALL state that the error code table is append-only: existing codes must never be renumbered or removed.
