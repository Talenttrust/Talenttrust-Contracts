---
title: "Enforce client authorization and caller binding on release_milestone"
labels: ["type:security", "area:milestones", "stack:soroban", "priority:high"]
type: Task
---

## Description

`Escrow::release_milestone` in `contracts/escrow/src/lib.rs` accepts only `(contract_id, milestone_index)` and never calls `require_auth()` nor checks the caller against `contract.client`. Any account can release any funded milestone to the freelancer, contradicting the README claim that "releases require the recorded client". This is a critical authorization gap in the escrow state machine.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Add a `caller: Address` parameter to `release_milestone`, call `caller.require_auth()`, and reject with `EscrowError::UnauthorizedRole` unless `caller == contract.client`.
- Preserve all existing invariants: `AlreadyReleased` guard via `DataKey::MilestoneReleased`, `InsufficientFunds` check, and `check_accounting_invariant`.
- Invariant: no milestone may transition to released without a verified client signature; `total_deposited == released_amount + refunded_amount + available_balance` must continue to hold.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b feature/release-milestone-auth`
- Implement changes:
  - `contracts/escrow/src/lib.rs`
  - Tests: `contracts/escrow/src/test/release_authorization.rs`
  - Docs: `docs/escrow/access-control.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): require client auth on release_milestone
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Require depositor authorization in deposit_funds"
labels: ["type:security", "area:escrow", "stack:soroban", "priority:high"]
type: Task
---

## Description

`Escrow::deposit_funds` in `contracts/escrow/src/lib.rs` mutates `total_deposited` and the contract status without any `require_auth()` call. Although it does not move tokens itself, the absence of caller authentication lets unauthorized parties drive `Created -> PartiallyFunded -> Funded` transitions and emit misleading `created`/audit events, weakening the fail-closed model.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Add a `depositor: Address` parameter, call `depositor.require_auth()`, and require `depositor == contract.client`.
- Keep `DepositMode::ExactTotal` and `DepositMode::Incremental` logic intact, including `ExactDepositRequired` and `DepositWouldExceedTotal` guards.
- Invariant: only the recorded client may fund a contract; deposits still cannot exceed the milestone total.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b feature/deposit-auth`
- Implement changes:
  - `contracts/escrow/src/lib.rs`
  - Tests: `contracts/escrow/src/test/deposit_authorization.rs`
  - Docs: `docs/escrow/access-control.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): require depositor auth in deposit_funds
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Implement protocol fee accounting on milestone release (protocol_fee_bps)"
labels: ["type:feature", "area:fees", "stack:soroban", "priority:high"]
type: Task
---

## Description

The README and `.kiro/specs/fee-accounting-tests` describe a configurable `protocol_fee_bps` deducted from each `release_milestone`, with accumulated fees tracked in treasury, but `contracts/escrow/src/lib.rs` contains no fee logic — `release_milestone` credits the full milestone amount and `DataKey::ProtocolFeeBps` / `DataKey::AccumulatedProtocolFees` are declared yet unused. Implement fee calculation, net-amount accounting, and treasury accumulation.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Add `set_protocol_fee_bps(admin, bps)` (admin-gated via `require_admin`) storing under `DataKey::ProtocolFeeBps`; clamp to a max of 1000 bps (10%) per the fee spec.
- In `release_milestone`, compute `fee = milestone_amount * bps / 10_000` (round down), `net = milestone_amount - fee`, add `fee` to `DataKey::AccumulatedProtocolFees`, and credit `net` to the freelancer accounting.
- Invariant: `fee + net == milestone_amount` after rounding; accumulated fees never exceed total released; default `bps == 0` pays the full amount.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b feature/protocol-fee-accounting`
- Implement changes:
  - `contracts/escrow/src/protocol_fees.rs`
  - Tests: `contracts/escrow/src/test/protocol_fees.rs`
  - Docs: `docs/escrow/FUNDING_ACCOUNTING.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): add protocol_fee_bps deduction on release
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Add protocol fee treasury withdrawal with admin authorization"
labels: ["type:feature", "area:fees", "stack:soroban", "priority:medium"]
type: Task
---

## Description

Once `DataKey::AccumulatedProtocolFees` is populated by milestone releases, there is no entrypoint to withdraw collected protocol fees. Add an admin-gated `withdraw_protocol_fees` that zeroes the accumulator and emits an auditable event, completing the fee lifecycle referenced in `docs/escrow/FUNDING_ACCOUNTING.md`.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Implement `withdraw_protocol_fees(admin, recipient)` requiring `require_admin` and `admin.require_auth()`; reject when the accumulator is zero.
- Reset `DataKey::AccumulatedProtocolFees` to 0 atomically and publish a `fee_wd` event with `(recipient, amount, timestamp)`.
- Invariant: total withdrawn over the contract lifetime equals total fees accrued; no withdrawal possible while paused/emergency.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b feature/fee-treasury-withdrawal`
- Implement changes:
  - `contracts/escrow/src/protocol_fees.rs`
  - Tests: `contracts/escrow/src/test/protocol_fees.rs`
  - Docs: `docs/escrow/FUNDING_ACCOUNTING.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): add admin protocol fee withdrawal
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Implement dispute lifecycle: raise_dispute and resolve_dispute with arbiter payouts"
labels: ["type:feature", "area:disputes", "stack:soroban", "priority:high"]
type: Task
---

## Description

`ContractStatus::Disputed` exists and `docs/escrow/dispute-resolution.md` documents resolution types (FullRefund, PartialRefund 70/30, FullPayout, Split), yet `contracts/escrow/src/lib.rs` exposes no dispute entrypoints. Implement `raise_dispute` and `resolve_dispute` so contract parties can escalate and an arbiter can settle escrowed balances.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- `raise_dispute(contract_id, caller)`: require `caller` is client or freelancer, only valid from `Funded`/`PartiallyFunded`, transition to `Disputed`, emit audit event.
- `resolve_dispute(contract_id, arbiter, resolution)`: require `arbiter == contract.arbiter` (must be set), apply the documented payout splits against the available balance, then transition to `Completed`/`Refunded`.
- Invariant: post-resolution `released_amount + refunded_amount == total_deposited`; `check_accounting_invariant` holds; resolution rejected when not `Disputed`.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b feature/dispute-lifecycle`
- Implement changes:
  - `contracts/escrow/src/dispute.rs`
  - Tests: `contracts/escrow/src/test/dispute.rs`
  - Docs: `docs/escrow/dispute-resolution.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): add raise_dispute and resolve_dispute
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Add arbiter assignment to escrow contracts"
labels: ["type:feature", "area:disputes", "stack:soroban", "priority:medium"]
type: Task
---

## Description

`EscrowContractData.arbiter` is always set to `None` in `create_contract` and there is no entrypoint to set it, making dispute resolution impossible. Add a mechanism for the client and freelancer to agree on an arbiter so `resolve_dispute` has an authorized settler.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Implement `assign_arbiter(contract_id, caller, arbiter)` requiring `caller.require_auth()` and that `caller` is client or freelancer; persist into `EscrowContractData.arbiter`.
- Reject if an arbiter is already set, if `arbiter` equals client or freelancer, or if status is past `Funded`.
- Invariant: arbiter is set at most once and is distinct from both parties; assignment blocked while paused.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b feature/arbiter-assignment`
- Implement changes:
  - `contracts/escrow/src/dispute.rs`
  - Tests: `contracts/escrow/src/test/dispute.rs`
  - Docs: `docs/escrow/dispute-workflow.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): add arbiter assignment entrypoint
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Implement refund_unreleased_milestones to back orphaned refund tests"
labels: ["type:feature", "area:milestones", "stack:soroban", "priority:high"]
type: Task
---

## Description

`contracts/escrow/src/refund.rs` is a test file calling `client.refund_unreleased_milestones(&contract_id, &refund_ids)`, but no such function exists in `contracts/escrow/src/lib.rs` and the file is not wired into the test module. Implement per-milestone refunds for unreleased milestones with the documented `EmptyRefundRequest` and `DuplicateMilestoneInRefund` guards.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Implement `refund_unreleased_milestones(contract_id, caller, milestone_indices: Vec<u32>)` requiring client auth, returning the total refunded amount.
- Reject empty requests (`EmptyRefundRequest`), duplicates (`DuplicateMilestoneInRefund`), already-released milestones (`AlreadyReleased`), and out-of-balance refunds (`InsufficientFunds`).
- Transition to `Refunded` when all unreleased milestones are refunded; otherwise keep `Funded`.
- Invariant: `refunded_amount` increases only for unreleased milestones; `check_accounting_invariant` holds.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b feature/refund-unreleased-milestones`
- Implement changes:
  - `contracts/escrow/src/refund_impl.rs`
  - Tests: `contracts/escrow/src/test/refund_milestone.rs`
  - Docs: `docs/escrow/milestone_schedule.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): implement refund_unreleased_milestones
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Implement two-step admin transfer for governance"
labels: ["type:feature", "area:governance", "stack:soroban", "priority:high"]
type: Task
---

## Description

The README states "governance changes use a one-time initialization plus a two-step admin transfer", but `contracts/escrow/src/lib.rs` only exposes `initialize` and a private `require_admin`; there is no admin transfer path. Implement a propose/accept two-step transfer to prevent transferring admin to an inaccessible address.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Add `propose_admin_transfer(current_admin, new_admin)` (admin-gated) storing a pending admin, and `accept_admin_transfer(new_admin)` requiring `new_admin.require_auth()` to finalize.
- Add `get_pending_admin` read accessor; clearing the pending entry on accept or on a new proposal.
- Invariant: admin changes only after the proposed account self-accepts; current admin retains control until acceptance.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b feature/two-step-admin-transfer`
- Implement changes:
  - `contracts/escrow/src/governance.rs`
  - Tests: `contracts/escrow/src/test/governance.rs`
  - Docs: `docs/escrow/governance-security.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): add two-step admin transfer
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Implement client migration using PendingClientMigration storage key"
labels: ["type:feature", "area:migration", "stack:soroban", "priority:medium"]
type: Task
---

## Description

`DataKey::PendingClientMigration(u32)` and TTL constants `PENDING_MIGRATION_TTL_LEDGERS` / `PENDING_MIGRATION_BUMP_THRESHOLD` exist in `types.rs` and `ttl.rs`, and `CLIENT_MIGRATION_IMPLEMENTATION.md` documents the feature, but `contracts/escrow/src/lib.rs` has no migration entrypoints. Implement a propose/accept client migration so the client role on a contract can be reassigned safely.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Implement `propose_client_migration(contract_id, current_client, new_client)` and `accept_client_migration(contract_id, new_client)` using `DataKey::PendingClientMigration` in temporary storage with the TTL helpers in `ttl.rs`.
- Reject when `new_client == freelancer`, when contract is `Completed`/`Cancelled`/`Refunded`, or when the pending migration has expired.
- Invariant: `EscrowContractData.client` updates only after the new client accepts a live (non-expired) proposal.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b feature/client-migration`
- Implement changes:
  - `contracts/escrow/src/migration.rs`
  - Tests: `contracts/escrow/src/test/client_migration.rs`
  - Docs: `docs/escrow/migration.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): add propose/accept client migration
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Implement finalize_contract closure with immutable close metadata"
labels: ["type:feature", "area:lifecycle", "stack:soroban", "priority:medium"]
type: Task
---

## Description

The README "Escrow closure finalization" section describes a `finalize_contract` that records immutable close metadata (timestamp, finalizer, summary) and is allowed only from `Completed` or `Disputed`, but no such function exists in `contracts/escrow/src/lib.rs`. Implement finalization to lock a contract's terminal record.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Implement `finalize_contract(contract_id, finalizer)` requiring `finalizer` be client, freelancer, or arbiter, allowed only from `Completed` or `Disputed`.
- Persist a finalization record (finalizer, timestamp, summary) and reject any subsequent mutating call referencing the contract with `AlreadyFinalized`.
- Invariant: a finalized contract is immutable; finalization is idempotent-guarded.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b feature/finalize-contract`
- Implement changes:
  - `contracts/escrow/src/finalize.rs`
  - Tests: `contracts/escrow/src/test/lifecycle.rs`
  - Docs: `docs/escrow/contract.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): add finalize_contract closure record
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Implement milestone approval expiry with grace period"
labels: ["type:feature", "area:milestones", "stack:soroban", "priority:medium"]
type: Task
---

## Description

`MilestoneApprovals` (client/freelancer/arbiter flags) and `DataKey::MilestoneApprovals(u32, u32)` are declared but unused, and TTL constants `PENDING_APPROVAL_TTL_LEDGERS` / `PENDING_APPROVAL_BUMP_THRESHOLD` exist in `ttl.rs` for a feature that is not wired in. Implement a milestone approval flow with an expiry grace period feeding `release_milestone`.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Implement `approve_milestone(contract_id, milestone_index, caller)` recording `MilestoneApprovals` in temporary storage with `PENDING_APPROVAL_TTL_LEDGERS`.
- Require both client and freelancer approval (or arbiter override) before `release_milestone` succeeds; expired approvals must be treated as absent.
- Invariant: a release is only valid against a live approval set; expired approvals are auto-evicted via TTL and yield `None`.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b feature/milestone-approval-expiry`
- Implement changes:
  - `contracts/escrow/src/approvals.rs`
  - Tests: `contracts/escrow/src/test/approval_expiry.rs`
  - Docs: `docs/escrow/milestone-validation.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): add milestone approval expiry flow
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Add get_contract_summary indexer view using ContractSummary schema"
labels: ["type:feature", "area:storage", "stack:soroban", "priority:low"]
type: Task
---

## Description

`ContractSummary`, `MilestoneSummary`, and `CONTRACT_SUMMARY_SCHEMA_VERSION` are fully defined in `types.rs` for indexer consumption, but no entrypoint in `contracts/escrow/src/lib.rs` produces them. Add a read-only `get_contract_summary` so off-chain indexers receive a versioned, denormalized view.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Implement `get_contract_summary(contract_id) -> ContractSummary` populating `schema_version = CONTRACT_SUMMARY_SCHEMA_VERSION`, per-milestone `released`/`refunded` flags from `DataKey::MilestoneReleased`, and `refundable_balance = total_deposited - released - refunded`.
- Must be a pure read (not blocked by pause) and panic `ContractNotFound` for unknown ids.
- Invariant: `funded_amount`, `released_amount`, and `refundable_balance` reconcile with the stored `EscrowContractData`.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b feature/contract-summary-view`
- Implement changes:
  - `contracts/escrow/src/lib.rs`
  - Tests: `contracts/escrow/src/test/summary.rs`
  - Docs: `docs/escrow/state-persistence.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): add get_contract_summary indexer view
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Add governed protocol parameter setter and readiness checklist wiring"
labels: ["type:feature", "area:governance", "stack:soroban", "priority:medium"]
type: Task
---

## Description

`ReadinessChecklist.governed_params_set` and `MainnetReadinessInfo.governed_params_set` exist but are never set to `true` because no governance parameter setter mutates the checklist in `contracts/escrow/src/lib.rs`. Add an admin entrypoint to set governed parameters (e.g. protocol fee, max escrow caps) and flip the readiness flag.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Implement `set_governed_params(admin, ...)` (admin-gated) that persists parameters and sets `checklist.governed_params_set = true` under `DataKey::ReadinessChecklist`.
- Ensure `get_mainnet_readiness_info` reflects the updated checklist.
- Invariant: `governed_params_set` becomes `true` only after a successful admin-authorized parameter set.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b feature/governed-params-setter`
- Implement changes:
  - `contracts/escrow/src/governance.rs`
  - Tests: `contracts/escrow/src/test/mainnet_readiness.rs`
  - Docs: `docs/escrow/mainnet-readiness.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): add governed params setter wiring readiness
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Wire orphaned test modules (deposit, release, refund, create_contract) into mod test"
labels: ["type:test", "area:escrow", "stack:soroban", "priority:high"]
type: Task
---

## Description

`contracts/escrow/src/test/mod.rs` only declares `emergency_controls` and `pause_controls`, so `cargo test` runs just 31 tests. The test files `deposit.rs`, `release.rs`, `refund.rs`, and `create_contract.rs` at the crate root reference functions like `refund_unreleased_milestones` and `get_milestones` with outdated signatures and are never compiled. Reconcile and wire these suites into the test module.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Move/align the orphaned suites under `contracts/escrow/src/test/` and declare them in `mod.rs`; update signatures to match the current `EscrowClient` API.
- Fix calls such as `release_milestone(&id, &0)` to match the authorized signature introduced by the auth hardening work.
- Acceptance: `cargo test` discovers and passes the migrated suites; no orphaned `.rs` test files remain at the crate root.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b test/wire-orphaned-suites`
- Implement changes:
  - `contracts/escrow/src/test/mod.rs`
  - Tests: `contracts/escrow/src/test/deposit.rs`
  - Docs: `docs/escrow/tests.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
test(escrow): wire orphaned deposit/release/refund suites
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Wire proptest and fuzz_test modules into the build"
labels: ["type:test", "area:escrow", "stack:soroban", "priority:medium"]
type: Task
---

## Description

`proptest` is declared as a dev-dependency in `contracts/escrow/Cargo.toml` and `contracts/escrow/src/proptest.rs` plus `fuzz_test.rs` exist, but neither is referenced from `lib.rs`, so no property-based tests run. Wire these modules in and ensure their generators target real entrypoints.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Add `#[cfg(test)] mod proptest;` and `#[cfg(test)] mod fuzz_test;` to `lib.rs` (or under the test module) and fix any compilation drift.
- Property generators should exercise milestone amount arrays, deposit sequences, and release/refund orderings, asserting `check_accounting_invariant` never panics unexpectedly.
- Acceptance: `cargo test` executes the property suites; cases shrink to minimal counterexamples on failure.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b test/wire-proptest-fuzz`
- Implement changes:
  - `contracts/escrow/src/lib.rs`
  - Tests: `contracts/escrow/src/proptest.rs`
  - Docs: `docs/escrow/fuzzing.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
test(escrow): enable proptest and fuzz_test modules
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Add accounting invariant property tests across deposit/release/refund sequences"
labels: ["type:test", "area:escrow", "stack:soroban", "priority:high"]
type: Task
---

## Description

`Escrow::check_accounting_invariant` enforces `total_deposited == released_amount + refunded_amount + available_balance` but is only exercised by happy-path emergency/pause tests today. Add property tests that drive randomized sequences of `deposit_funds`, `release_milestone`, and refunds to prove the invariant never breaks.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Generate random milestone arrays and random valid operation orderings; after each operation assert the contract's stored amounts satisfy the invariant and `available_balance >= 0`.
- Include adversarial sequences (over-release attempts, double refund) expecting the documented errors rather than invariant violations.
- Acceptance: at least 256 generated cases per property; failures shrink to minimal repro.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b test/accounting-invariant-properties`
- Implement changes:
  - `contracts/escrow/src/proptest.rs`
  - Tests: `contracts/escrow/src/test/accounting_invariants.rs`
  - Docs: `docs/escrow/FUNDING_ACCOUNTING.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
test(escrow): add accounting invariant property tests
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Add negative-path tests for issue_reputation authorization and status gating"
labels: ["type:test", "area:reputation", "stack:soroban", "priority:medium"]
type: Task
---

## Description

`Escrow::issue_reputation` enforces caller-is-client (`UnauthorizedRole`), freelancer match (`FreelancerMismatch`), `Completed` status (`NotCompleted`), rating bounds 1..=5 (`InvalidRating`), and single issuance (`ReputationAlreadyIssued`), yet only one happy-path reputation test exists and `test_reputation.rs` is orphaned. Add targeted negative-path coverage for each guard.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Use `assert_contract_error` (from `test/mod.rs`) with `try_issue_reputation` to assert each error code: wrong caller, wrong freelancer, non-completed contract, rating 0 and 6, and a second issuance.
- Verify `ReputationRecord` aggregation (`completed_contracts`, `total_rating`, `last_rating`) and `PendingReputationCredits` decrement on success.
- Acceptance: every `EscrowError` branch in `issue_reputation` is covered.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b test/reputation-negative-paths`
- Implement changes:
  - `contracts/escrow/src/test/reputation.rs`
  - Tests: `contracts/escrow/src/test/reputation.rs`
  - Docs: `docs/escrow/REPUTATION.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
test(escrow): cover issue_reputation negative paths
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Add storage TTL tests for transient approval and migration entries"
labels: ["type:test", "area:storage", "stack:soroban", "priority:medium"]
type: Task
---

## Description

The TTL helpers in `contracts/escrow/src/ttl.rs` (`store_with_ttl`, `read_if_live`, `extend_if_below_threshold`, `remove_transient`) are all marked `#[allow(dead_code)]` and untested. Add tests using `env.ledger()` manipulation to prove TTL semantics for `PENDING_APPROVAL_TTL_LEDGERS` and `PENDING_MIGRATION_TTL_LEDGERS`.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Verify `read_if_live` returns `Some` before expiry and `None` after advancing the ledger past the TTL; verify `extend_if_below_threshold` bumps a live entry and returns `false` for a missing key.
- Cover both approval and migration TTL constants and `LEDGERS_PER_DAY` math.
- Acceptance: deterministic tests that advance ledger sequence and assert auto-eviction behavior.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b test/storage-ttl`
- Implement changes:
  - `contracts/escrow/src/ttl.rs`
  - Tests: `contracts/escrow/src/test/ttl_tests.rs`
  - Docs: `docs/escrow/storage-ttl.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
test(escrow): add transient TTL expiry tests
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Add tests for create_contract bounds (MAX_MILESTONES and MAX_TOTAL_ESCROW_STROOPS)"
labels: ["type:test", "area:milestones", "stack:soroban", "priority:medium"]
type: Task
---

## Description

`create_contract` enforces `TooManyMilestones` when `milestone_amounts.len() > MAX_MILESTONES` (10), `InvalidMilestoneAmount` for non-positive amounts or totals over `MAX_TOTAL_ESCROW_STROOPS`, and `PotentialOverflow` via `safe_add_amounts`. The orphaned `test_bounds.rs` covers some of this but is not compiled. Add wired tests for all bounds.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Assert: exactly 10 milestones succeeds; 11 panics `TooManyMilestones`; zero/negative milestone panics `InvalidMilestoneAmount`; total over `MAX_TOTAL_ESCROW_STROOPS` panics; near-`i128::MAX` amounts panic `PotentialOverflow`.
- Assert `InvalidParticipant` when `client == freelancer` and `EmptyMilestones` for an empty vector.
- Acceptance: each guard in `create_contract` has a dedicated case.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b test/create-contract-bounds`
- Implement changes:
  - `contracts/escrow/src/test/milestone_schedule.rs`
  - Tests: `contracts/escrow/src/test/milestone_schedule.rs`
  - Docs: `docs/escrow/milestone-validation.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
test(escrow): cover create_contract bounds and overflow
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Harden cancel_contract to reject cancellation of disputed and refunded contracts"
labels: ["type:security", "area:lifecycle", "stack:soroban", "priority:medium"]
type: Task
---

## Description

`Escrow::cancel_contract` only rejects `Cancelled` (`AlreadyCancelled`) and `Completed` (`InvalidStatusTransition`) statuses; a contract in `Disputed` or `Refunded` can still be cancelled by either party, which can strand or double-resolve escrowed funds. Tighten the allowed source states for cancellation.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Reject cancellation from `Disputed` and `Refunded` with `InvalidStatusTransition`; document the exact set of cancellable states (`Created`, `PartiallyFunded`, `Funded`).
- Keep the client-or-freelancer authorization check and `check_accounting_invariant` enforcement.
- Invariant: `cancel_contract` is a no-op-or-error from any terminal or in-resolution state.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b security/cancel-state-guardrails`
- Implement changes:
  - `contracts/escrow/src/lib.rs`
  - Tests: `contracts/escrow/src/test/cancel_contract.rs`
  - Docs: `docs/escrow/status-transition-guardrails.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
fix(escrow): restrict cancel_contract source states
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Persist and enforce a single canonical max escrow cap across create_contract and deposit"
labels: ["type:security", "area:escrow", "stack:soroban", "priority:medium"]
type: Task
---

## Description

`lib.rs` defines two conflicting caps: `MAX_TOTAL_ESCROW_STROOPS = 10_000_000_000_000` (enforced in `create_contract`) and `MAINNET_MAX_TOTAL_ESCROW_PER_CONTRACT_STROOPS = 1_000_000_000_000_000` (reported by `get_mainnet_readiness_info` but never enforced). `amount_validation.rs` adds a third bound `MAX_SINGLE_AMOUNT_STROOPS`. Consolidate to one enforced, governable cap to avoid silent mismatch between reported and enforced limits.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Choose a single canonical per-contract cap, enforce it in `create_contract` and as the ceiling in `deposit_funds`, and have `get_mainnet_readiness_info` report the same value.
- Document the relationship between per-contract cap and `amount_validation::MAX_SINGLE_AMOUNT_STROOPS`.
- Invariant: the cap reported by readiness equals the cap enforced at create/deposit time.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b security/canonical-escrow-cap`
- Implement changes:
  - `contracts/escrow/src/lib.rs`
  - Tests: `contracts/escrow/src/test/mainnet_readiness.rs`
  - Docs: `docs/escrow/mainnet-readiness.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
fix(escrow): unify enforced and reported escrow caps
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Replace permissive crate-level clippy allows with targeted lint exceptions"
labels: ["type:security", "area:escrow", "stack:rust", "priority:low"]
type: Task
---

## Description

`contracts/escrow/src/lib.rs` opens with ~25 blanket `#![allow(...)]` attributes (including `dead_code`, `unused_imports`, `unused_variables`), which masks exactly the kind of dead code that left `proptest.rs`, `fuzz_test.rs`, and the TTL helpers silently unused. CI runs `cargo clippy -- -D warnings`, so these allows hide real defects. Narrow them to module/item scope.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Remove crate-wide `dead_code`, `unused_imports`, `unused_variables` allows; address resulting warnings or scope allows to specific items with justification comments.
- Keep only stylistic clippy allows that are genuinely required by the soroban-sdk macro expansions.
- Invariant: `cargo clippy --workspace --all-targets -- -D warnings` passes with no blanket suppression of dead code.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b security/tighten-clippy-allows`
- Implement changes:
  - `contracts/escrow/src/lib.rs`
  - Tests: `contracts/escrow/src/test/security.rs`
  - Docs: `docs/escrow/SECURITY.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
refactor(escrow): scope clippy allows to surface dead code
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Guard NextContractId allocation against overflow and reuse"
labels: ["type:security", "area:storage", "stack:soroban", "priority:low"]
type: Task
---

## Description

`create_contract` reads `DataKey::NextContractId`, defaults to 1, and writes `id + 1` using unchecked `u32` addition. Near `u32::MAX` this wraps and could collide with an existing `DataKey::Contract(id)`, silently overwriting an active escrow. Add overflow-safe id allocation.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Use checked addition for `id + 1` and panic with a dedicated overflow error if the id space is exhausted.
- Optionally assert `DataKey::Contract(id)` is unset before writing to detect any collision defensively.
- Invariant: contract ids are strictly monotonic and never reused; allocation fails closed at the boundary.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b security/contract-id-overflow`
- Implement changes:
  - `contracts/escrow/src/lib.rs`
  - Tests: `contracts/escrow/src/test/storage.rs`
  - Docs: `docs/escrow/state-persistence.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
fix(escrow): overflow-safe NextContractId allocation
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Bump persistent storage TTL for active escrow contracts to prevent eviction"
labels: ["type:security", "area:storage", "stack:soroban", "priority:high"]
type: Task
---

## Description

Active escrows are written via `env.storage().persistent().set(&DataKey::Contract(id), ...)` but `lib.rs` never calls `extend_ttl` on persistent entries, while `ttl.rs` only handles temporary storage. Long-running contracts risk persistent-entry eviction (and fund-state loss) on Soroban. Add explicit TTL extension on contract reads/writes.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- On each mutating access to `DataKey::Contract(id)` (and related milestone keys), call `extend_ttl` with a deterministic bump threshold/extend-to policy documented in `ttl.rs`.
- Ensure the policy covers `DataKey::MilestoneReleased`, `DataKey::Reputation`, and `DataKey::AccumulatedProtocolFees`.
- Invariant: any contract touched within its TTL window remains live; eviction cannot strand active escrow accounting.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b security/persistent-ttl-bump`
- Implement changes:
  - `contracts/escrow/src/ttl.rs`
  - Tests: `contracts/escrow/src/test/persistence.rs`
  - Docs: `docs/escrow/storage-ttl.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): bump persistent TTL on active contracts
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Centralize amount validation by wiring amount_validation into create_contract and deposit_funds"
labels: ["type:enhancement", "area:escrow", "stack:rust", "priority:medium"]
type: Task
---

## Description

`contracts/escrow/src/amount_validation.rs` provides `validate_milestone_amounts`, `validate_deposit_amount`, and stroop-precision helpers, but `create_contract` and `deposit_funds` in `lib.rs` re-implement their own ad-hoc loops with `safe_add_amounts`. Consolidate to a single validation path to remove drift between the two checks.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Route `create_contract` milestone validation through `validate_milestone_amounts` and `deposit_funds` through `validate_deposit_amount`, mapping `AmountValidationError` variants to the corresponding `EscrowError` codes.
- Preserve existing error semantics (`InvalidMilestoneAmount`, `PotentialOverflow`, `DepositWouldExceedTotal`).
- Invariant: validation behavior is identical before and after, with one source of truth.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b enhancement/centralize-amount-validation`
- Implement changes:
  - `contracts/escrow/src/lib.rs`
  - Tests: `contracts/escrow/src/test/input_sanitization_amounts.rs`
  - Docs: `docs/escrow/milestone-validation.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
refactor(escrow): route amounts through amount_validation
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Emit structured events for deposit_funds and release fee deductions"
labels: ["type:enhancement", "area:escrow", "stack:soroban", "priority:low"]
type: Task
---

## Description

`deposit_funds` emits only an audit event on status change and no dedicated deposit event, unlike `create_contract` (`created`) and `release_milestone` (`released`). Indexers cannot reconstruct funding history. Add a structured `deposit` event and a fee event aligned with the protocol fee work.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Emit `(symbol_short!("deposit"), contract_id)` with `(amount, new_total_deposited, new_status, timestamp)` on every successful deposit.
- When protocol fees are active, emit a `fee` topic with `(contract_id, milestone_index, fee, net, timestamp)` from `release_milestone`.
- Invariant: every balance-changing operation emits exactly one structured event reconstructable by an indexer.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b enhancement/deposit-and-fee-events`
- Implement changes:
  - `contracts/escrow/src/lib.rs`
  - Tests: `contracts/escrow/src/test/flows.rs`
  - Docs: `docs/escrow/contract.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): emit structured deposit and fee events
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Refactor repeated admin-load boilerplate into a single helper"
labels: ["type:enhancement", "area:access-control", "stack:rust", "priority:low"]
type: Task
---

## Description

`pause`, `unpause`, `activate_emergency_pause`, and `resolve_emergency` in `lib.rs` each repeat the same `storage().persistent().get(&DataKey::Admin).unwrap_or_else(...)` + `admin.require_auth()` block, and the existing `require_admin(env, caller)` helper is never called. Extract a single `load_and_auth_admin(env) -> Address` helper and reuse it.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Introduce `fn load_and_auth_admin(env: &Env) -> Address` that loads the admin (panicking `NotInitialized` if missing) and calls `require_auth`, then use it in all four control entrypoints.
- Either wire `require_admin` into a caller-supplied flow or remove it to eliminate dead code.
- Invariant: admin authorization semantics are unchanged; no duplicated admin-load logic remains.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b enhancement/admin-auth-helper`
- Implement changes:
  - `contracts/escrow/src/lib.rs`
  - Tests: `contracts/escrow/src/test/access_control.rs`
  - Docs: `docs/escrow/access-control.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
refactor(escrow): extract load_and_auth_admin helper
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Prevent issue_reputation self-rating and add SelfRating enforcement"
labels: ["type:enhancement", "area:reputation", "stack:soroban", "priority:medium"]
type: Task
---

## Description

`EscrowError::SelfRating` is defined in `types.rs` but never used. `issue_reputation` checks `caller == contract.client` and `freelancer == contract.freelancer`, but does not explicitly prevent a degenerate contract where roles were migrated such that client and freelancer collapse to the same address from self-inflating reputation. Enforce the `SelfRating` guard.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- In `issue_reputation`, reject with `EscrowError::SelfRating` if `contract.client == contract.freelancer` (defense-in-depth alongside create-time `InvalidParticipant`).
- Confirm the guard interacts correctly with any future client-migration feature.
- Invariant: a single principal can never both issue and receive reputation on the same contract.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b enhancement/reputation-self-rating-guard`
- Implement changes:
  - `contracts/escrow/src/lib.rs`
  - Tests: `contracts/escrow/src/test/reputation.rs`
  - Docs: `docs/escrow/REPUTATION.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): enforce SelfRating guard in issue_reputation
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Add average-rating accessor derived from ReputationRecord"
labels: ["type:enhancement", "area:reputation", "stack:soroban", "priority:low"]
type: Task
---

## Description

`ReputationRecord` stores `completed_contracts`, `total_rating`, and `last_rating`, but `get_reputation` returns the raw record with no convenience for consumers needing an average. Add a read-only accessor that computes the average rating safely (handling zero completed contracts).

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Implement `get_average_rating(freelancer) -> Option<i128>` returning `total_rating / completed_contracts` (scaled, e.g. by 100 for two-decimal precision) or `None` when no contracts completed.
- Avoid division by zero and document the scaling factor.
- Invariant: average is `None` exactly when `completed_contracts == 0`; otherwise within the 1..=5 (scaled) range.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b enhancement/average-rating-accessor`
- Implement changes:
  - `contracts/escrow/src/lib.rs`
  - Tests: `contracts/escrow/src/test/reputation.rs`
  - Docs: `docs/escrow/REPUTATION.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): add get_average_rating accessor
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Add audit events for protocol fee changes and admin transfers"
labels: ["type:enhancement", "area:governance", "stack:soroban", "priority:low"]
type: Task
---

## Description

The contract emits audit events for contract lifecycle transitions via `emit_audit_event`, and init/pause/emergency events, but governance-sensitive changes like fee-rate updates and admin transfers (to be added) have no standardized audit trail. Extend the event model to cover all privileged parameter changes.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Emit events for `set_protocol_fee_bps` (`old_bps`, `new_bps`, `admin`, `timestamp`) and for admin transfer proposal/acceptance.
- Keep event topic naming consistent with existing `symbol_short!` topics (`init`, `paused`, `emergency`).
- Invariant: every privileged state change produces a distinguishable, parseable event.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b enhancement/governance-audit-events`
- Implement changes:
  - `contracts/escrow/src/governance.rs`
  - Tests: `contracts/escrow/src/test/governance.rs`
  - Docs: `docs/escrow/governance-security.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
feat(escrow): audit events for fee and admin changes
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Reconcile README and docs with actually implemented escrow entrypoints"
labels: ["type:docs", "area:escrow", "stack:soroban", "priority:high"]
type: Task
---

## Description

The README and several `docs/escrow/*` files describe features that do not exist in `contracts/escrow/src/lib.rs`: two-step admin transfer, `finalize_contract`, protocol fee model (`protocol_fee_bps`, `protocol_fee_account`), and a `migrate_state`/`StateV1`/`StateV2` flow. This documentation drift is misleading to reviewers and integrators. Audit and reconcile docs against the real API surface.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Cross-check every documented function against the `pub fn` surface in `lib.rs`; mark unimplemented features as "Planned" or remove them, and link to the tracking issues created for them.
- Correct the `release_milestone` description (currently claims client-required) until the auth fix lands.
- Acceptance: no doc claims an entrypoint or guarantee not present in code (or it is clearly labeled Planned).
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b docs/reconcile-readme-api`
- Implement changes:
  - `docs/escrow/contract.md`
  - Tests: `contracts/escrow/src/test/summary.rs`
  - Docs: `docs/escrow/README.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
docs(escrow): reconcile README with implemented API
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Document the canonical DataKey storage layout and key versioning"
labels: ["type:docs", "area:storage", "stack:soroban", "priority:medium"]
type: Task
---

## Description

`types.rs` defines a `DataKey` enum spanning admin/pause/emergency, contract storage, reputation, client migration, and protocol/governance keys, but several keys (`MilestoneApprovals`, `PendingClientMigration`, `ProtocolFeeBps`, `AccumulatedProtocolFees`) are unused and undocumented. Produce an authoritative storage-key reference describing each key, its value type, and persistent vs temporary placement.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- For each `DataKey` variant: document the stored value type, storage class (persistent/temporary), TTL policy (referencing `ttl.rs`), and which entrypoints read/write it.
- Flag keys that are currently declared-but-unused and link to the issues that implement them.
- Acceptance: `docs/escrow/state-persistence.md` matches the `DataKey` enum exactly.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b docs/datakey-storage-layout`
- Implement changes:
  - `docs/escrow/state-persistence.md`
  - Tests: `contracts/escrow/src/test/storage.rs`
  - Docs: `docs/escrow/state-persistence.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
docs(escrow): document canonical DataKey layout
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Document the ContractStatus state machine and allowed transitions"
labels: ["type:docs", "area:lifecycle", "stack:soroban", "priority:medium"]
type: Task
---

## Description

`ContractStatus` defines eight states (`Created`, `Accepted`, `Funded`, `Completed`, `Disputed`, `Cancelled`, `Refunded`, `PartiallyFunded`) but `Accepted` is never assigned and the legal transitions are scattered implicitly across `deposit_funds`, `release_milestone`, `cancel_contract`, and `issue_reputation`. Produce a definitive state-transition diagram and table.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Document every legal transition with the triggering entrypoint and guard error (e.g. `Created -> PartiallyFunded` via `deposit_funds`, `Funded -> Completed` when all milestones released).
- Explicitly note that `Accepted` is currently unused and either propose its use or recommend removal.
- Acceptance: the diagram in `docs/escrow/status-transition-guardrails.md` matches the implemented transitions in `lib.rs`.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b docs/status-state-machine`
- Implement changes:
  - `docs/escrow/status-transition-guardrails.md`
  - Tests: `contracts/escrow/src/test/lifecycle.rs`
  - Docs: `docs/escrow/status-transition-guardrails.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
docs(escrow): document ContractStatus state machine
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Document the EscrowError catalog with trigger conditions"
labels: ["type:docs", "area:escrow", "stack:soroban", "priority:low"]
type: Task
---

## Description

`EscrowError` in `types.rs` defines 43 variants, several of which are unused (`Accepted`-related, `SelfRating`, `CommentTooLong`, `EmptyComment`, `MilestonesAlreadyReleased`, `NotReadyForFinalization`). Reviewers and integrators have no single reference mapping error codes to the conditions that raise them. Produce a complete error-code reference.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- For each `EscrowError` variant, document its numeric code, the entrypoint(s) that raise it, and the precise trigger condition; clearly mark variants that are currently unused/reserved.
- Cross-link to the relevant entrypoint docs and to issues that will start using reserved codes.
- Acceptance: the catalog matches the enum's numbering (1..=43) exactly.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b docs/error-catalog`
- Implement changes:
  - `docs/escrow/SECURITY.md`
  - Tests: `contracts/escrow/src/test/security.rs`
  - Docs: `docs/escrow/SECURITY.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
docs(escrow): document EscrowError code catalog
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
++++++
---
title: "Add explicit double-release and double-refund regression tests for MilestoneReleased flags"
labels: ["type:test", "area:milestones", "stack:soroban", "priority:high"]
type: Task
---

## Description

`release_milestone` guards re-release via `DataKey::MilestoneReleased(contract_id, milestone_index)` raising `AlreadyReleased`, and the all-released check drives the `Completed` transition. These guards are central to fund safety but the only wired tests are pause/emergency happy paths. Add dedicated regression tests for double-release and the release-then-refund interaction.

## Requirements and context

- Scoped to TalentTrust `escrow` Soroban contract (`contracts/escrow`).
- Assert releasing the same milestone twice panics `AlreadyReleased`; releasing all milestones flips status to `Completed` and increments `PendingReputationCredits`.
- Assert attempting to refund an already-released milestone fails, and releasing a refunded milestone fails (once refund is implemented), using `try_*` + `assert_contract_error`.
- Acceptance: per-milestone release/refund flags are proven idempotent and mutually exclusive.
- Must be secure, tested, and documented.

## Suggested execution

- Fork the repo and create a branch:
  - `git checkout -b test/double-release-refund-regression`
- Implement changes:
  - `contracts/escrow/src/test/release_authorization.rs`
  - Tests: `contracts/escrow/src/test/release_authorization.rs`
  - Docs: `docs/escrow/tests.md`
  - Include rustdoc/NatSpec-style doc comments on public functions
  - Validate security assumptions (auth, overflow, fail-closed state machine, storage TTL, fee accounting)

## Test and commit

- Run tests: `cargo test`
- Cover edge cases (unauthorized callers, double release/refund, expired approvals, fee rounding, paused state)
- Include test output and security notes in the PR

## Example commit message

```
test(escrow): regression tests for double release/refund
```

## Guidelines

- Minimum 95% test coverage on new/changed code
- Clear documentation
- Timeframe: 96 hours from assignment
