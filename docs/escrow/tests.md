# Escrow Tests

The active escrow unit-test tree is declared in `contracts/escrow/src/test/mod.rs`.
At the time of this documentation pass it includes:

- `pause_controls`
- `emergency_controls`
- `lifecycle`
- `summary`

Tests are located in:
- Main test module: [`contracts/escrow/src/test.rs`](../../contracts/escrow/src/test.rs)
- Test suites: [`contracts/escrow/src/test/`](../../contracts/escrow/src/test/)

## Test Organization

The test suite is organized into modular test files under `contracts/escrow/src/test/`:

### Core Functionality Tests
- **`deposit.rs`** - Deposit fund accumulation, state transitions, and validation
- **`release.rs`** - Milestone release flows, authorization, and double-spending prevention
- **`refund.rs`** - Refund logic, balance tracking, and state transitions
- **`create_contract.rs`** - Contract creation, milestone validation, and participant checks

### Security & Access Control Tests
- **`access_control.rs`** - Role-based authorization checks
- **`security.rs`** - Security-critical operations and attack prevention
- **`approval_expiry.rs`** - Approval TTL expiry and fail-closed behavior
- **`emergency_controls.rs`** - Emergency pause and recovery mechanisms
- **`pause_controls.rs`** - Contract pause/unpause functionality

### State Management Tests
- **`lifecycle.rs`** - Full contract lifecycle state transitions
- **`flows.rs`** - End-to-end workflow scenarios
- **`persistence.rs`** - Storage persistence and data integrity
- **`storage.rs`** - Storage TTL and data eviction

### Input Validation Tests
- **`input_sanitization_amounts.rs`** - Amount validation and overflow prevention
- **`input_sanitization_identities.rs`** - Address validation and participant checks

### Advanced Features Tests
- **`milestone_schedule.rs`** - Milestone scheduling and sequencing
- **`governance.rs`** - Governance and arbiter functionality
- **`timeout_tests.rs`** - Timeout handling and expiry
- **`client_migration.rs`** - Client address migration
- **`performance.rs`** - Performance benchmarks and gas optimization
- **`mainnet_readiness.rs`** - Production readiness checks

### Basic Tests
- **`hello.rs`** - Smoke tests and connectivity checks

## Migrated Test Suites (v0.3.0)

The following test suites were previously orphaned at the crate root and have been migrated to `contracts/escrow/src/test/` with updated signatures to match the current EscrowClient API:

### Deposit Tests (`test/deposit.rs`)

#### `accumulates_deposits_without_exceeding_total`
- **Purpose:** Validates that deposits accumulate correctly and transition to Funded status when fully funded.
- **Setup:** Create contract with 1,200 stroops total, deposit 600 twice.
- **Assertions:** First deposit keeps status as Created; second deposit transitions to Funded.
- **Security:** Validates state transition logic and funded_amount tracking accuracy.

#### `rejects_zero_deposit`
- **Purpose:** Ensures zero-amount deposits are rejected.
- **Setup:** Attempt to deposit 0 stroops.
- **Assertion:** Panics with AmountMustBePositive error.
- **Security:** Prevents dust attacks and invalid state transitions.

#### `rejects_overfunding`
- **Purpose:** Prevents deposits exceeding total milestone amount.
- **Setup:** Attempt to deposit 1,300 stroops when total is 1,200.
- **Assertion:** Panics (overfunding prevention).
- **Security:** Ensures contract accounting integrity.

#### `rejects_deposit_after_full_refund_resolution`
- **Purpose:** Validates fail-closed state machine after refund.
- **Setup:** Deposit funds, refund all milestones, attempt another deposit.
- **Assertion:** Panics with InvalidState error.
- **Security:** Prevents re-funding of resolved contracts.

### Release Tests (`test/release.rs`)

#### `releases_funded_milestones_and_completes_when_all_are_released`
- **Purpose:** Validates sequential milestone release and completion transition.
- **Setup:** Fund contract, approve and release all 3 milestones sequentially.
- **Assertions:** Each release updates released_amount; final release transitions to Completed; refundable balance is tracked correctly.
- **Security:** Validates authorization checks, amount tracking, and state transitions.

#### `rejects_release_without_sufficient_balance`
- **Purpose:** Prevents overdraft attacks.
- **Setup:** Deposit only 100 stroops, attempt to release 200 stroop milestone.
- **Assertion:** Panics with InsufficientFunds error.
- **Security:** Validates balance checks before release.

#### `rejects_release_of_invalid_milestone`
- **Purpose:** Prevents out-of-bounds access.
- **Setup:** Attempt to release milestone index 3 when only 3 milestones exist (0-2).
- **Assertion:** Panics with IndexOutOfBounds error.
- **Security:** Validates milestone index bounds.

#### `rejects_releasing_refunded_milestone`
- **Purpose:** Prevents double-spending.
- **Setup:** Refund milestone 1, then attempt to release it.
- **Assertion:** Panics with AlreadyRefunded error.
- **Security:** Validates milestone state before release.

#### `rejects_releasing_same_milestone_twice`
- **Purpose:** Prevents double-spending.
- **Setup:** Release milestone 0, then attempt to release it again.
- **Assertion:** Panics with MilestoneAlreadyReleased error.
- **Security:** Validates milestone released flag.

### Refund Tests (`test/refund.rs`)

#### `refunds_selected_unreleased_milestones_and_preserves_remaining_balance`
- **Purpose:** Validates partial refund logic and balance tracking.
- **Setup:** Release milestone 0, refund milestone 1, keep milestone 2 available.
- **Assertions:** Refunded amount is correct; milestone flags are set; refundable balance is accurate.
- **Security:** Ensures refund accounting accuracy and state integrity.

#### `marks_contract_refunded_when_all_unreleased_milestones_are_refunded`
- **Purpose:** Validates state transition to Refunded status.
- **Setup:** Refund all 3 milestones without releasing any.
- **Assertions:** Contract status is Refunded; refundable balance is zero.
- **Security:** Confirms proper state transition and finalization.

#### `rejects_empty_refund_request`
- **Purpose:** Prevents invalid state transitions.
- **Setup:** Attempt to refund with empty milestone indices vector.
- **Assertion:** Panics with EmptyRefundRequest error.
- **Security:** Validates input sanitization.

#### `rejects_duplicate_milestones_in_single_refund`
- **Purpose:** Prevents double-refund attacks.
- **Setup:** Attempt to refund milestone 1 twice in same call.
- **Assertion:** Panics with DuplicateMilestoneInRefund error.
- **Security:** Validates input sanitization and prevents accounting errors.

#### `rejects_refunding_released_milestone`
- **Purpose:** Prevents double-spending.
- **Setup:** Release milestone 0, then attempt to refund it.
- **Assertion:** Panics with AlreadyReleased error.
- **Security:** Validates milestone state before refund.

#### `rejects_refunding_same_milestone_twice`
- **Purpose:** Prevents double-refund attacks.
- **Setup:** Refund milestone 2, then attempt to refund it again.
- **Assertion:** Panics with AlreadyRefunded error.
- **Security:** Validates milestone refunded flag.

#### `rejects_refund_when_balance_is_not_available`
- **Purpose:** Prevents overdraft attacks.
- **Setup:** Deposit only 200 stroops, attempt to refund 400 stroop milestone.
- **Assertion:** Panics with InsufficientFunds error.
- **Security:** Validates balance checks before refund.

### Create Contract Tests (`test/create_contract.rs`)

#### `creates_contract_and_persists_milestones`
- **Purpose:** Validates contract creation and milestone persistence.
- **Setup:** Create contract with 3 milestones.
- **Assertions:** Contract ID is 1; status is Created; all milestone amounts are stored correctly.
- **Security:** Ensures contract initialization and data integrity.

#### `rejects_empty_milestones`
- **Purpose:** Prevents invalid contract initialization.
- **Setup:** Attempt to create contract with empty milestones vector.
- **Assertion:** Panics with EmptyMilestones error.
- **Security:** Validates input sanitization.

#### `rejects_zero_amount_milestone`
- **Purpose:** Prevents dust attacks.
- **Setup:** Attempt to create contract with 0 stroop milestone.
- **Assertion:** Panics with InvalidMilestoneAmount error.
- **Security:** Validates milestone amount constraints.

#### `rejects_same_participants`
- **Purpose:** Prevents self-dealing.
- **Setup:** Attempt to create contract where client and freelancer are the same address.
- **Assertion:** Panics with InvalidParticipants error.
- **Security:** Validates participant uniqueness.

### Migration Notes

**API Signature Updates:**
- `deposit_funds(contract_id, amount)` → `deposit_funds(contract_id, caller, amount)`
- `release_milestone(contract_id, milestone_index)` → `release_milestone(contract_id, caller, milestone_index)`
- Added `approve_milestone_release(contract_id, caller, milestone_index)` calls before releases
- `create_contract(client, freelancer, milestones)` → `create_contract(client, freelancer, arbiter, milestones, release_authorization)`

**Security Enhancements:**
- All tests now include proper authorization with `caller` parameter
- Approval workflow is tested (approve before release)
- ReleaseAuthorization mode is specified (ClientOnly for default tests)
- Comprehensive rustdoc comments document security assumptions

## Test Categories

### 1. Connectivity & Smoke Tests

#### `test_hello`
- **Purpose:** Verify contract registration and basic method invocation works.
- **Calls:** `hello("World")`
- **Assertion:** Result equals `"World"`.
- **Importance:** CI/CD health check; ensures test harness itself is functional.

---

### 2. Contract Lifecycle: Creation

#### `test_create_contract_returns_id`
- **Purpose:** Verify first contract receives ID = 1.
- **Setup:** Call `create_contract(client, freelancer, [200, 400, 600])`.
- **Assertion:** Returned ID is `1`.
- **Importance:** Confirms auto-incrementing ID counter starts at 1.

#### `test_create_contract_ids_increment`
- **Purpose:** Verify sequential contract IDs increment.
- **Setup:** Create two contracts.
- **Assertions:** First ID = `1`, second ID = `2`.
- **Importance:** Proves IDs are unique and predictable.

#### `test_create_contract_rejects_empty_milestones`
- **Purpose:** Verify contract creation panics if no milestones provided.
- **Setup:** Call `create_contract(..., [])`.
- **Assertion:** Panics with `"at least one milestone required"`.
- **Importance:** Ensures invalid (empty) contracts cannot be created.

---

### 3. Contract Lifecycle: Deposit

#### `test_deposit_funds_transitions_to_funded`
- **Purpose:** Verify contract state changes from `Created` to `Funded`.
- **Setup:** Create contract, call `deposit_funds`.
- **Assertion:** Contract is now in `Funded` state (implicit; it allows milestone release).
- **Importance:** Gate function works correctly for state transition.

#### `test_deposit_funds_returns_true`
- **Purpose:** Verify return value is `true`.
- **Setup:** Create contract, call `deposit_funds`.
- **Assertion:** Result is `true`.
- **Importance:** Confirms expected return type.

#### `test_deposit_rejects_non_positive_amount`
- **Purpose:** Verify deposit rejects amount <= 0.
- **Setup:** Call `deposit_funds(cid, 0)`.
- **Assertion:** Panics with `"deposit amount must be positive"`.
- **Importance:** Prevents degenerate deposits.

#### `test_deposit_rejects_already_funded_contract`
- **Purpose:** Verify calling `deposit_funds` twice panics.
- **Setup:** Create contract, call `deposit_funds` twice.
- **Assertion:** Second call panics with `"contract not in Created status"`.
- **Importance:** Prevents re-funding (state guard).

---

### 4. Contract Lifecycle: Release Milestone

#### `test_release_milestone_returns_true`
- **Purpose:** Verify return value is `true`.
- **Setup:** Create and fund contract, call `release_milestone`.
- **Assertion:** Result is `true`.
- **Importance:** Confirms expected return type.

#### `test_release_all_milestones_succeeds`
- **Purpose:** Verify multiple milestones can be released in sequence.
- **Setup:** Create contract with 3 milestones, fund, release all 3.
- **Assertion:** All 3 calls return `true`.
- **Importance:** Confirms milestone independence (no interdependencies).

#### `test_release_already_released_milestone_panics`
- **Purpose:** Verify releasing the same milestone twice panics.
- **Setup:** Create contract, fund, release milestone 0 twice.
- **Assertion:** Second release panics with `"milestone already released"`.
- **Importance:** Double-release prevention.

#### `test_release_out_of_range_milestone_panics`
- **Purpose:** Verify out-of-range index is rejected.
- **Setup:** Create contract with 1 milestone, try to release milestone 99.
- **Assertion:** Panics with `"milestone_id out of range"`.
- **Importance:** Index boundary check.

#### `test_release_on_created_contract_panics`
- **Purpose:** Verify release on non-funded contract panics.
- **Setup:** Create contract, call `release_milestone` without `deposit_funds`.
- **Assertion:** Panics with `"contract not in Funded status"`.
- **Importance:** State precondition (can't release before funded).

---

### 5. Contract Lifecycle: Completion

#### `test_complete_contract_returns_true`
- **Purpose:** Verify return value is `true`.
- **Setup:** Fund, release all milestones, call `complete_contract`.
- **Assertion:** Result is `true`.
- **Importance:** Confirms expected return type.

#### `test_complete_contract_rejects_unreleased_milestones`
- **Purpose:** Verify `complete_contract` panics if any milestone unreleased.
- **Setup:** Fund, release 1 of 2 milestones, call `complete_contract`.
- **Assertion:** Panics with `"all milestones must be released before completing"`.
- **Importance:** Final-settlement gate.

#### `test_complete_contract_rejects_no_milestones_released`
- **Purpose:** Verify `complete_contract` panics if no milestones released.
- **Setup:** Fund, call `complete_contract` without releasing any milestone.
- **Assertion:** Panics with `"all milestones must be released before completing"`.
- **Importance:** Strongest test of final-settlement gate.

#### `test_complete_contract_rejects_created_status`
- **Purpose:** Verify `complete_contract` rejects contract not in `Funded` status.
- **Setup:** Create contract, call `complete_contract` (without `deposit_funds`).
- **Assertion:** Panics with `"contract not in Funded status"`.
- **Importance:** State precondition.

---

### 6. Reputation Issuance: Happy Path

#### `test_issue_reputation_full_happy_path`
- **Purpose:** End-to-end success case.
- **Setup:** Complete full lifecycle: create -> fund -> release all -> complete -> issue.
- **Assertion:** `issue_reputation(cid, 5)` returns `true`.
- **Importance:** Verifies entire workflow succeeds.

#### `test_issue_reputation_minimum_rating`
- **Purpose:** Verify rating = 1 is accepted.
- **Setup:** Complete contract, call `issue_reputation(..., 1)`.
- **Assertion:** Returns `true`.
- **Importance:** Boundary check (lower bound).

#### `test_issue_reputation_maximum_rating`
- **Purpose:** Verify rating = 5 is accepted.
- **Setup:** Complete contract, call `issue_reputation(..., 5)`.
- **Assertion:** Returns `true`.
- **Importance:** Boundary check (upper bound).

#### `test_issue_reputation_single_milestone_contract`
- **Purpose:** Verify works with minimal contract (1 milestone).
- **Setup:** Complete a 1-milestone contract, issue reputation.
- **Assertion:** Returns `true`.
- **Importance:** Minimal case verification.

---

### 7. Constraint 1: Contract Existence

#### `test_reputation_panics_contract_not_found`
- **Purpose:** Verify `issue_reputation` panics for non-existent contract.
- **Setup:** Call `issue_reputation(999, 5)` (contract 999 was never created).
- **Assertion:** Panics with `"contract not found"`.
- **Importance:** Lookup validation.

---

### 8. Constraint 2: Completion Gate

#### `test_reputation_panics_when_status_is_created`
- **Purpose:** Verify reputation issuance rejected if contract is `Created`.
- **Setup:** Create contract, immediately try to issue reputation.
- **Assertion:** Panics with `"reputation can only be issued after contract completion"`.
- **Importance:** Completion gate (contract never funded).

#### `test_reputation_panics_when_status_is_funded`
- **Purpose:** Verify reputation issuance rejected if contract is `Funded`.
- **Setup:** Create and fund, try to issue reputation without completing.
- **Assertion:** Panics with `"reputation can only be issued after contract completion"`.
- **Importance:** Completion gate (contract not yet completed).

#### `test_reputation_panics_after_partial_milestones_not_completed`
- **Purpose:** Verify reputation rejected if some milestones released but `complete_contract` not called.
- **Setup:** Fund, release 2/3 milestones, try to issue reputation.
- **Assertion:** Panics with `"reputation can only be issued after contract completion"`.
- **Importance:** Verifies completion is gating factor, not just milestone count.

---

### 9. Constraint 3: Final Settlement

#### `test_reputation_panics_when_milestone_unreleased_before_complete`
- **Purpose:** Verify final settlement gate.
- **Setup:** Fund, release 1/2 milestones, try to complete (this test verifies `complete_contract` prevents the illegal state).
- **Assertion:** `complete_contract` panics with `"all milestones must be released before completing"`.
- **Importance:** Proves `complete_contract` itself enforces final settlement, making the check inside `issue_reputation` a safety redundancy.

---

### 10. Constraint 4: No Double Issuance

#### `test_reputation_panics_on_double_issuance`
- **Purpose:** Verify reputation can only be issued once per contract.
- **Setup:** Complete contract, issue reputation, try to issue again.
- **Assertion:** Second issuance panics with `"reputation already issued for this contract"`.
- **Importance:** Critical security test (prevents exploit of issuance logic).

#### `test_reputation_panics_on_double_issuance_different_rating`
- **Purpose:** Verify double-issuance is blocked even with different rating.
- **Setup:** Complete contract, issue with rating 5, try to issue with rating 3.
- **Assertion:** Panics with `"reputation already issued for this contract"`.
- **Importance:** Proves flag-based check is independent of input, preventing clever workarounds.

---

### 11. Constraint 5: Valid Rating

#### `test_reputation_panics_rating_zero`
- **Purpose:** Verify rating 0 is rejected (below range).
- **Setup:** Complete contract, call `issue_reputation(..., 0)`.
- **Assertion:** Panics with `"rating must be between 1 and 5"`.
- **Importance:** Lower bound check.

#### `test_reputation_panics_rating_six`
- **Purpose:** Verify rating 6 is rejected (above range).
- **Setup:** Complete contract, call `issue_reputation(..., 6)`.
- **Assertion:** Panics with `"rating must be between 1 and 5"`.
- **Importance:** Upper bound check.

#### `test_reputation_panics_rating_max_u32`
- **Purpose:** Verify large rating values are rejected.
- **Setup:** Complete contract, call `issue_reputation(..., u32::MAX)`.
- **Assertion:** Panics with `"rating must be between 1 and 5"`.
- **Importance:** Extreme input test.

---

### 12. Contract Isolation

#### `test_reputation_only_for_completed_contract_not_other`
- **Purpose:** Verify one contract's status doesn't affect another.
- **Setup:** Create two contracts: one funded (incomplete), one completed. Try to issue reputation for funded contract.
- **Assertion:** Funded contract rejects reputation; completed contract allows it.
- **Importance:** Proves contract state is properly isolated in persistent storage.

#### `test_each_contract_gets_independent_reputation_flag`
- **Purpose:** Verify each contract has its own issuance flag.
- **Setup:** Create two completed contracts, issue reputation for both.
- **Assertion:** Both succeed; second single-issuance flag is independent of first.
- **Importance:** Proves `DataKey::ReputationIssued(contract_id)` is scoped per contract.

---

### 13. Non-existent Contract Errors

#### `test_deposit_panics_contract_not_found`
- **Purpose:** Verify `deposit_funds` panics if contract doesn't exist.
- **Setup:** Call `deposit_funds(999, 100)`.
- **Assertion:** Panics with `"contract not found"`.
- **Importance:** Lookup validation for deposit path.

#### `test_release_panics_contract_not_found`
- **Purpose:** Verify `release_milestone` panics if contract doesn't exist.
- **Setup:** Call `release_milestone(999, 0)`.
- **Assertion:** Panics with `"contract not found"`.
- **Importance:** Lookup validation for release path.

---

## Test Execution & CI/CD

### Running Tests Locally

```bash
cd talenttrust-contracts
cargo test
```

**Expected Output Examples:**
```
running 45 tests

test test_hello ... ok
test test_create_contract_returns_id ... ok
test test_deposit_funds_transitions_to_funded ... ok
test test_reputation_panics_on_double_issuance ... ok
...

test result: ok. 45 passed; 0 failed; 0 ignored
```

### CI/CD Validation

GitHub Actions automatically runs `cargo test` on every push to `main` and pull requests. All tests must pass before merging.

---

## Coverage Matrix

| Category | Tests | Coverage |
|----------|-------|----------|
| Smoke Tests | 1 | Connectivity |
| Contract Creation | 3 | Happy path + empty milestone error |
| Deposit | 4 | Return value, state transition, non-positive, re-fund |
| Release Milestone | 5 | Return value, multiple, double-release, out-of-range, wrong status |
| Complete Contract | 4 | Return value, unreleased guard, all unreleased, wrong status |
| Reputation (Happy Path) | 4 | Full flow, min/max rating, single milestone |
| Constraint 1 | 1 | Contract existence |
| Constraint 2 | 3 | Status Created, Funded, partial no-complete |
| Constraint 3 | 1 | Final settlement gate (tested via complete_contract) |
| Constraint 4 | 2 | Double-issuance, different rating |
| Constraint 5 | 3 | Rating 0, 6, max_u32 |
| Isolation | 2 | Multi-contract, independent flags |
| Error Paths | 2 | Non-existent contracts |
| **TOTAL** | **45** | **Comprehensive** |

---

## Security Testing Approach

1. **Constraint Completeness:** Each of the 5 reputation issuance constraints has >= 1 dedicated test.
2. **Ordering Verification:** Constraints are checked in order; tests verify early panics don't mask later ones.
3. **Boundary Testing:** Min/max values tested (ratings 1 & 5, empty milestones, max_u32).
4. **Idempotency & Replay:** Double-issuance and re-fund tests verify immutability.
5. **State Isolation:** Multi-contract tests prove persistence storage key scoping is correct.
6. **Authorization (implicit):** All tests use `env.mock_all_auths()`, confirming auth checks don't accidentally block valid callers.

---

## Future Test Enhancements

- **Dispute Status -> Cancellation Flow:** Test full lifecycle of dispute leading to cancellation.
- **Stress Testing:** Large number of milestones (e.g., 1000s) to verify scalability.
- **Asset Integration:** Integration tests with actual Stellar asset contracts (currently mocked).
- **Fuzzing:** Randomized input testing for rating values, contract IDs, milestone counts.
- **Performance:** Benchmark contract invocation times for gas cost estimation.
- **Timeout Expiry:** Once timeout logic is implemented, add `TimeoutExpired` cancellation tests.

---

## Cancellation Tests (Added in v0.2.0)

### 7. Contract Cancellation Path

The cancellation tests cover all policy-defined scenarios for `cancel_contract`.

#### `test_cancel_contract_in_created_state_by_client`
- **Purpose:** Client can unilaterally cancel before any funds are deposited.
- **Setup:** Create contract (no deposit).
- **Calls:** `cancel_contract(1, client_addr)`
- **Assertions:** Returns `true`; contract status is `Cancelled`.
- **Importance:** Validates that either party can exit freely before funding.

#### `test_cancel_contract_in_created_state_by_freelancer`
- **Purpose:** Freelancer can unilaterally cancel before any funds are deposited.
- **Setup:** Create contract (no deposit).
- **Calls:** `cancel_contract(1, freelancer_addr)`
- **Assertions:** Returns `true`; contract status is `Cancelled`.
- **Importance:** Validates that freelancer can also exit early (reason: `FreelancerInitiated`).

#### `test_cancel_contract_in_created_state_unauthorized`
- **Purpose:** Third party cannot cancel a contract they're not party to.
- **Setup:** Create contract, use unrelated address.
- **Calls:** `cancel_contract(1, unauthorized_addr)`
- **Assertion:** Panics with `"Caller must be client or freelancer to cancel in Created state"`.
- **Importance:** Validates strict access control on cancellation.

#### `test_cancel_contract_in_funded_state_by_client_no_release`
- **Purpose:** Client can cancel a funded contract if no milestones have been released.
- **Setup:** Create and fund contract.
- **Calls:** `cancel_contract(1, client_addr)`
- **Assertions:** Returns `true`; contract status is `Cancelled`.
- **Importance:** Validates client refund path before work is delivered.

#### `test_cancel_contract_in_funded_state_client_after_release`
- **Purpose:** Client cannot cancel if any milestone has already been released.
- **Setup:** Create contract with 2 milestones, fund, release milestone 0.
- **Calls:** `cancel_contract(1, client_addr)`
- **Assertion:** Panics with `"Client cannot cancel after milestones have been released"`.
- **Importance:** Prevents client from taking back payments after freelancer delivered work.

#### `test_cancel_contract_by_arbiter_in_funded_state`
- **Purpose:** Arbiter can cancel a funded contract at any time.
- **Setup:** Create contract with arbiter, deposit funds.
- **Calls:** `cancel_contract(1, arbiter_addr)`
- **Assertions:** Returns `true`; contract status is `Cancelled`.
- **Importance:** Validates arbiter authority for dispute resolution scenarios.

#### `test_cancel_contract_already_cancelled`
- **Purpose:** A cancelled contract cannot be cancelled again.
- **Setup:** Create and cancel contract, then try again.
- **Calls:** second `cancel_contract(1, client_addr)`
- **Assertion:** Panics with `"Contract already cancelled"`.
- **Importance:** Prevents double-cancellation and audit trail corruption.

#### `test_cancel_contract_completed`
- **Purpose:** A completed contract cannot be cancelled.
- **Setup:** Create, fund, release all milestones (contract becomes Completed).
- **Calls:** `cancel_contract(1, client_addr)`
- **Assertion:** Panics with `"Cannot cancel a completed contract"`.
- **Importance:** Protects finalized contracts from retroactive cancellation.

#### `test_cancel_contract_freelancer_mutual_agreement`
- **Purpose:** Freelancer can initiate cancellation in funded state (mutual agreement path).
- **Setup:** Create and fund contract.
- **Calls:** `cancel_contract(1, freelancer_addr)`
- **Assertions:** Returns `true`; contract status is `Cancelled`.
- **Importance:** Validates freelancer's ability to exit an engagement in progress.

#### `test_get_contract`
- **Purpose:** Verify contract data retrieval works correctly.
- **Setup:** Create contract with arbiter.
- **Calls:** `get_contract(1)`
- **Assertions:** Client, freelancer, arbiter, status, and milestone count match.
- **Importance:** Validates query API and confirms storage correctness.

#### `test_get_nonexistent_contract`
- **Purpose:** Verify querying a contract that doesn't exist panics.
- **Setup:** None.
- **Calls:** `get_contract(999)`
- **Assertion:** Panics with `"Contract not found"`.
- **Importance:** Validates error handling for invalid contract IDs.

---

## Test Design Principles

1. **Happy Path First:** Each tested function has at least one test showing the successful path.
2. **Guard-Driven Coverage:** Every constraint or guard in the contract has a corresponding negative test.
3. **Boundary Testing:** Min/max values tested (ratings 1 & 5, empty milestones, max_u32).
4. **Idempotency & Replay:** Double-issuance and re-fund tests verify immutability.
5. **State Isolation:** Multi-contract tests prove persistence storage key scoping is correct.
6. **Authorization (implicit):** All tests use `env.mock_all_auths()`, confirming auth checks don't accidentally block valid callers.
7. **Cancellation Path Coverage:** All cancellation policies tested — every authorized and unauthorized path.

---

## Version

- **Version:** 0.3.0
- **Last Updated:** 2026-05-28
- **Test Count:** 31+ (additional tests in migrated suites)
- **New in v0.3.0:** Migrated orphaned test suites (deposit, release, refund, create_contract) from crate root to test/ directory with updated API signatures
- **New in v0.2.0:** Contract cancellation path — 9 tests covering all status/role combinations
