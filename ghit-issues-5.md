---
type: Feature
title: "Fix get_pending_governance_admin to decode the PendingAdminProposal struct it stores"
labels: type:security, area:governance, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Fix get_pending_governance_admin to decode the PendingAdminProposal struct it stores

### Description
`propose_governance_admin_impl` in [`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs) writes a **`PendingAdminProposal { proposed, proposed_at_ledger }`** struct under `DataKey::PendingAdmin`. But the public reader `get_pending_governance_admin` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) reads the very same key as a bare `Address`. These two types do not share a wire layout, so the reader either panics on decode or silently returns `None`, leaving off-chain governance dashboards blind to a live admin-rotation proposal.

This issue makes the reader decode the stored `PendingAdminProposal` and return `proposal.proposed`, matching the in-module `get_pending_governance_admin_impl` that already does this correctly.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Change `get_pending_governance_admin` to load `Option<PendingAdminProposal>` and map to the proposed `Address`, delegating to `get_pending_governance_admin_impl` rather than reading the raw key.
- Confirm there is exactly one storage shape for `DataKey::PendingAdmin` across propose/accept/read paths.
- Add a typed accessor (e.g. `get_pending_governance_admin_proposed_at`) so indexers can also read the timelock anchor ledger.
- Preserve return type `Option<Address>` so existing client SDKs keep compiling.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-pending-admin-decode`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) and [`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/admin_auth_helper.rs`](contracts/escrow/src/test/admin_auth_helper.rs) — propose then read back the proposed address and the anchor ledger.
  - **Add documentation:** note the canonical `PendingAdmin` storage shape in [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the corrected reader.
  - Validate security: ensure no panic-on-decode path remains reachable from a read-only call.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover: no proposal present, a live proposal, and round-trip of the proposed address.
- Include the full `cargo test` output in the PR description.

### Example commit message
`fix: decode PendingAdminProposal in get_pending_governance_admin with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add the missing EscrowError variants referenced across governance, reputation, and evidence paths"
labels: type:security, area:errors, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add the missing EscrowError variants referenced across governance, reputation, and evidence paths

### Description
Several entrypoints panic with `EscrowError` variants that are **not declared** in the `EscrowError` enum in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs): `set_governed_params` uses `InvalidProtocolParameters`, `accept_governance_admin_impl` uses `TimelockNotElapsed`, `submit_work_evidence` uses `EvidenceTooLong`, and `issue_reputation` uses `EmptyComment` and `CommentTooLong`. The enum only runs up to `AmountMustBePositive = 30`. The contract cannot compile cleanly until these are added, and once added they must be **append-only** to keep client error codes stable.

This issue introduces the missing variants with explicit, append-only discriminants and documents their trigger conditions.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `TimelockNotElapsed`, `InvalidProtocolParameters`, `EvidenceTooLong`, `EmptyComment`, and `CommentTooLong` to `EscrowError` with fresh discriminants `31..=35` — never reusing or reordering existing codes.
- Cross-check every `panic_with_error(EscrowError::…)` site so no variant is referenced before it exists.
- Document each new variant with a `///` comment describing its trigger condition.
- Add a doc note that error discriminants are an external ABI and are append-only.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-escrowerror-missing-variants`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — extend the `EscrowError` enum.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/security.rs`](contracts/escrow/src/test/security.rs) — assert each variant is returned on its trigger condition.
  - **Add documentation:** extend the error catalog under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on each new variant.
  - Validate security: confirm discriminants are append-only and unique.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover each new variant's exact trigger path.
- Include the full `cargo test` output in the PR description.

### Example commit message
`fix: add missing append-only EscrowError variants with trigger tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Enforce the governed max_escrow_total_stroops cap in create_contract and deposit_funds"
labels: type:feature, area:governance, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Enforce the governed max_escrow_total_stroops cap in create_contract and deposit_funds

### Description
`set_governed_params` in [`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs) persists a `GovernedParameters { protocol_fee_bps, max_escrow_total_stroops }` and flips `governed_params_set` in the readiness checklist — but **nothing ever reads `max_escrow_total_stroops` back**. Contracts can be created and funded for any amount, so the governance cap is purely cosmetic.

This issue wires the cap into the money-flow paths: `create_contract` rejects a milestone total above the cap, and `deposit_funds` rejects a funded amount that would push the escrow past it.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Read `GovernedParameters.max_escrow_total_stroops` (when set and > 0) in [`contracts/escrow/src/create_contract.rs`](contracts/escrow/src/create_contract.rs) and reject milestone totals exceeding it.
- Apply the same ceiling in [`contracts/escrow/src/deposit.rs`](contracts/escrow/src/deposit.rs) against the post-deposit `funded_amount`.
- Use checked arithmetic for the aggregate milestone total before comparison.
- Return a clear typed error (reuse `InvalidProtocolParameters` or add a dedicated `EscrowCapExceeded` append-only variant) and document it.
- Treat an unset/zero cap as "no limit" to preserve existing behavior.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-enforce-escrow-cap`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/create_contract.rs`](contracts/escrow/src/create_contract.rs) and [`contracts/escrow/src/deposit.rs`](contracts/escrow/src/deposit.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/create_contract_bounds.rs`](contracts/escrow/src/test/create_contract_bounds.rs) — at, below, and above the cap.
  - **Add documentation:** describe the cap semantics under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the enforcement points.
  - Validate security: overflow-safe total computation and unset-cap fallthrough.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover: cap unset, exact-cap, over-cap on create, over-cap on incremental deposit.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat: enforce governed max escrow cap in create_contract and deposit_funds`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add admin-rotation timelock tests for accept_governance_admin's TimelockNotElapsed gate"
labels: type:test, area:governance, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add admin-rotation timelock tests for accept_governance_admin's TimelockNotElapsed gate

### Description
`accept_governance_admin_impl` in [`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs) enforces a treasury-rotation timelock: acceptance panics with `TimelockNotElapsed` until `ADMIN_ROTATION_MIN_DELAY_LEDGERS` (~2 days) have elapsed since the proposal's `proposed_at_ledger`. This is a critical safety rail with no dedicated test coverage exercising the ledger-advance boundary.

This issue adds tests that advance the ledger sequence around the delay boundary and assert the timelock is honored exactly.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Use Soroban's test ledger controls to set `sequence` at `delay - 1` (reject), `delay` (allow), and `delay + N` (allow).
- Assert acceptance before the delay panics with `TimelockNotElapsed`, and that acceptance at/after the delay rotates the admin and clears `PendingAdmin`.
- Assert the `admin/accepted` event payload carries the old and new admins.
- Cover the no-pending-proposal path returning `InvalidState`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-admin-rotation-timelock`
- Implement changes
  - **Write code in:** no production change expected; if a ledger-advance helper is missing, add it to [`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/admin_auth_helper.rs`](contracts/escrow/src/test/admin_auth_helper.rs).
  - **Add documentation:** note the timelock policy under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on any added helper.
  - Validate security: the boundary at exactly `delay` is allowed, not off-by-one.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover both sides of the delay boundary plus the missing-proposal path.
- Include the full `cargo test` output in the PR description.

### Example commit message
`test: cover admin-rotation timelock boundary in accept_governance_admin`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Document and test the basis-point scaling contract of get_average_rating"
labels: type:test, area:reputation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document and test the basis-point scaling contract of get_average_rating

### Description
`get_average_rating` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) returns `total_rating * 10_000 / completed_contracts` — a basis-point scaled average where a raw 5.0 returns `50_000`. This non-obvious scaling has no tests pinning the decimal contract, no test for the `None`-on-zero-contracts guard, and no overflow test for the `checked_mul(10_000)` path.

This issue locks the scaling behavior down with tests and a worked example in the docs so integrators correctly divide by `10_000`.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Test: single 5-star rating returns `50_000`; mixed ratings (e.g. 5 and 4 across two contracts) return `45_000`.
- Test: missing reputation record and `completed_contracts == 0` both return `None`.
- Test: a very large `total_rating` exercises the `checked_mul` guard (returns `None`, no panic).
- Add a docs example mapping basis points back to a 1–5 decimal.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-average-rating-scaling`
- Implement changes
  - **Write code in:** no production change unless the overflow guard needs hardening in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/reputation.rs`](contracts/escrow/src/test/reputation.rs).
  - **Add documentation:** add a scaling example under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) clarifying the scale factor.
  - Validate security: division-by-zero is impossible via the `None` guard.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover exact, mixed, zero-contract, and overflow inputs.
- Include the full `cargo test` output in the PR description.

### Example commit message
`test: pin basis-point scaling and guards of get_average_rating`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Make the PartialRefund 70/30 split deterministic and conserve the remainder stroop"
labels: type:security, area:dispute, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Make the PartialRefund 70/30 split deterministic and conserve the remainder stroop

### Description
`resolution_payouts` in [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) implements `DisputeResolution::PartialRefund` as `freelancer_payout = available * 30 / 100` and `client_payout = available - freelancer_payout`. Integer truncation means the freelancer is always rounded down and the leftover stroop is silently swept to the client. The doc comment says "70% to client and 30% to freelancer" but never states the rounding rule, so the split is under-specified and untested at non-divisible balances.

This issue specifies the rounding rule explicitly (remainder to client), proves conservation (`client + freelancer == available`), and tests boundary balances such as `1`, `2`, and `7`.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Keep the floor-division freelancer share but document that the remainder accrues to the client, and assert `client + freelancer == available` for every input.
- Add tests at indivisible balances (`available = 1, 2, 7, 99`) verifying exact payouts.
- Confirm `available == 0` yields `(0, 0)` with no panic.
- Reuse `safe_add_amounts`/checked math; never allow a negative payout.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-partial-refund-rounding`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) — clarify rounding and conservation.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/dispute.rs`](contracts/escrow/src/test/dispute.rs).
  - **Add documentation:** state the rounding rule in the `DisputeResolution::PartialRefund` docs.
  - Include NatSpec-style doc comments (`///`) on the split math.
  - Validate security: no value is created or destroyed across the split.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover indivisible balances and the zero-balance edge.
- Include the full `cargo test` output in the PR description.

### Example commit message
`fix: make PartialRefund split deterministic and conserving with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Extend TTL when reading milestone approvals in get_milestone_approvals"
labels: type:enhancement, area:storage-ttl, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Extend TTL when reading milestone approvals in get_milestone_approvals

### Description
`get_milestone_approvals` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) reads `DataKey::MilestoneApprovals(contract_id, milestone_index)` from temporary storage but, unlike the contract and milestone readers that call `ttl::extend_contract_ttl`, it does **not** bump the approval entry's TTL. A dApp that polls approval status while a party gathers the second MultiSig signature can therefore accelerate eviction toward the read window without renewing it, surfacing a confusing "approval vanished" state.

This issue makes the approval reader renew the temporary entry's TTL on access, consistent with the write path in `approvals::approve_milestone`.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- On a successful read, call `extend_ttl` with `PENDING_APPROVAL_BUMP_THRESHOLD`/`PENDING_APPROVAL_TTL_LEDGERS` from [`contracts/escrow/src/ttl.rs`](contracts/escrow/src/ttl.rs).
- Only extend when the entry exists; an absent/expired entry must still return `None` without writing.
- Keep the read-only-ish semantics documented: this is a storage-touching read, callers should not assume zero cost.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-approval-read-ttl`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/approval_expiry.rs`](contracts/escrow/src/test/approval_expiry.rs) — assert TTL renewal on read and `None` for absent entries.
  - **Add documentation:** note the renew-on-read behavior under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the reader.
  - Validate security: no write occurs for a missing entry.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover present-entry renewal and absent-entry no-op.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat: renew approval TTL on get_milestone_approvals reads`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Wire DepositMode (ExactTotal vs Incremental) into deposit_funds instead of leaving it dead"
labels: type:feature, area:deposit, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Wire DepositMode (ExactTotal vs Incremental) into deposit_funds instead of leaving it dead

### Description
The `DepositMode { ExactTotal, Incremental }` enum in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) is declared but never referenced. `deposit_funds` in [`contracts/escrow/src/deposit.rs`](contracts/escrow/src/deposit.rs) always behaves incrementally (it adds `amount` to `funded_amount` and only flips to `Funded` once the running total covers the milestone sum), with no way to require a single exact-total deposit.

This issue gives the enum meaning: a contract records its `DepositMode`, and `deposit_funds` rejects deposits that violate it — `ExactTotal` requires one deposit equal to the milestone total, `Incremental` keeps today's top-up behavior.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Persist a `DepositMode` per contract (set at `create_contract`, defaulting to `Incremental` to preserve current behavior).
- In `ExactTotal` mode, reject any deposit whose amount is not exactly the outstanding milestone total with a clear typed error.
- In `Incremental` mode, keep the existing accumulate-and-promote logic.
- Document the mode semantics and the default.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-deposit-mode`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/deposit.rs`](contracts/escrow/src/deposit.rs) and [`contracts/escrow/src/create_contract.rs`](contracts/escrow/src/create_contract.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/deposit.rs`](contracts/escrow/src/test/deposit.rs) — exact, under, over for both modes.
  - **Add documentation:** describe the modes under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the mode branch.
  - Validate security: incremental default keeps existing flows unbroken.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover both modes including rejection paths.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat: enforce DepositMode semantics in deposit_funds with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a get_work_evidence reader for milestone deliverable references"
labels: type:feature, area:work-evidence, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a get_work_evidence reader for milestone deliverable references

### Description
`submit_work_evidence` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) stores a deliverable reference into `Milestone.work_evidence` and emits an `evidence` event, but there is no targeted reader: clients must pull the entire milestone vector via `get_milestones` and index into it to find one milestone's evidence. There is no single-milestone accessor that returns `Option<String>`.

This issue adds `get_work_evidence(contract_id, milestone_index) -> Option<String>` with bounds checking, so a freelancer's submitted CID/URL can be fetched directly.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `get_work_evidence` returning `Option<String>` (None when unset or out of bounds — or a typed `IndexOutOfBounds`, pick one and document it).
- Extend milestone TTL on read, consistent with `get_milestones`.
- Do not require auth; evidence is a read-only view.
- Document the bounds and absent-evidence semantics.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-get-work-evidence`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/deposit.rs`](contracts/escrow/src/test/deposit.rs) or a dedicated evidence test module — set then read, absent, out-of-bounds.
  - **Add documentation:** add the accessor to the entrypoint reference under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the reader.
  - Validate security: out-of-bounds index is handled deterministically.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover set, unset, and out-of-bounds reads.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat: add get_work_evidence single-milestone reader with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Populate per-milestone funded_amount and refunded_amount so milestone state self-describes"
labels: type:feature, area:accounting, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Populate per-milestone funded_amount and refunded_amount so milestone state self-describes

### Description
The `Milestone` struct in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) has `funded_amount` and `refunded_amount` fields, but `create_contract`, `deposit_funds`, `release_milestone`, and `refund_unreleased_milestones` only ever set the contract-level totals and the boolean `released`/`refunded` flags — the per-milestone amount fields stay at `0`. Indexers and dispute logic cannot tell from a single milestone how much it actually received or returned.

This issue populates these fields as funds flow through each milestone, so `Milestone` is self-describing without cross-referencing contract-level aggregates.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- On refund, set `milestone.refunded_amount = milestone.amount` for each refunded milestone.
- On deposit (and/or release), set `milestone.funded_amount` to reflect coverage of that milestone's `amount`.
- Keep the sum of per-milestone fields consistent with the contract-level `funded_amount`/`refunded_amount`; add an invariant test.
- Use checked arithmetic for the per-milestone updates.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-milestone-amount-fields`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/deposit.rs`](contracts/escrow/src/deposit.rs) and the refund/release paths in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/release.rs`](contracts/escrow/src/test/release.rs) and [`contracts/escrow/src/test/deposit.rs`](contracts/escrow/src/test/deposit.rs).
  - **Add documentation:** describe per-milestone accounting under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the populated fields.
  - Validate security: per-milestone sums reconcile to contract totals.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover partial refunds and mixed release/refund milestones.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat: populate per-milestone funded and refunded amounts with invariants`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Make next_contract_id overflow-safe at u32::MAX instead of wrapping the id counter"
labels: type:security, area:id-allocation, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Make next_contract_id overflow-safe at u32::MAX instead of wrapping the id counter

### Description
`create_contract_impl` in [`contracts/escrow/src/create_contract.rs`](contracts/escrow/src/create_contract.rs) allocates an id via `next_contract_id`, then persists `id + 1` as the new `NextContractId`. When `id == u32::MAX`, `id + 1` wraps to `0` in release builds, after which the next allocation collides with low ids and would overwrite an existing contract. The documented `ContractIdOverflow` error is never actually produced by this path.

This issue makes the increment use `checked_add(1)` and panic with `ContractIdOverflow` at the ceiling, so the documented behavior matches reality.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Replace `id + 1` with `id.checked_add(1)`, panicking with `Error::ContractIdOverflow` on `None`.
- Keep the existing `ContractIdCollision` guard for already-occupied slots.
- Add a test that seeds `NextContractId = u32::MAX` and asserts the overflow error rather than a wrap.
- Confirm the happy-path allocation still increments normally.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-id-overflow-guard`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/create_contract.rs`](contracts/escrow/src/create_contract.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/contract_id_allocation.rs`](contracts/escrow/src/test/contract_id_allocation.rs).
  - **Add documentation:** note the overflow behavior in the create_contract docs.
  - Include NatSpec-style doc comments (`///`) on the increment.
  - Validate security: no wrap-to-zero allocation is reachable.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover the ceiling case and a normal allocation.
- Include the full `cargo test` output in the PR description.

### Example commit message
`fix: guard next_contract_id increment against u32 overflow`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Emit an approved event from approve_milestone_release for indexer visibility"
labels: type:enhancement, area:events, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Emit an approved event from approve_milestone_release for indexer visibility

### Description
`approve_milestone` in [`contracts/escrow/src/approvals.rs`](contracts/escrow/src/approvals.rs) records a client/freelancer/arbiter approval into temporary storage but emits **no event**. Off-chain indexers therefore cannot observe approval progress and have no signal that a MultiSig milestone now has one of its two required approvals — they only see the eventual release.

This issue emits a structured `approved` event capturing the contract id, milestone index, approver, and which role flag flipped.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Publish an event after a successful approval write with `(contract_id, milestone_index, approver, role, timestamp)`.
- Distinguish the role (client/freelancer/arbiter) so consumers can compute remaining MultiSig signatures.
- Do not emit on the duplicate-approval (`AlreadyApproved`) rejection path.
- Keep topic naming consistent with existing `symbol_short!` conventions.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-approval-event`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/approvals.rs`](contracts/escrow/src/approvals.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/approval_expiry.rs`](contracts/escrow/src/test/approval_expiry.rs) — assert event payload per role.
  - **Add documentation:** add the event to the event catalog under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) describing the emitted topic/payload.
  - Validate security: no event on rejected approvals.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover client, freelancer, and arbiter approval events.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat: emit approved event from approve_milestone_release with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Gate approve_milestone_release behind the pause and emergency controls"
labels: type:security, area:pause-controls, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Gate approve_milestone_release behind the pause and emergency controls

### Description
`approve_milestone_release` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) only calls `require_not_finalized` before delegating to `approvals::approve_milestone`; it never calls `require_not_paused`. While `release_milestone` itself is gated elsewhere, allowing approvals to accumulate during a pause or emergency lets parties stage a release the instant the freeze lifts, defeating part of the emergency-freeze guarantee.

This issue adds the pause/emergency gate to the approval entrypoint so no approval state mutates while the contract is frozen.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Call `Self::require_not_paused(&env)` at the top of `approve_milestone_release` before any storage write.
- Ensure paused returns `ContractPaused` and emergency returns `EmergencyActive`.
- Keep `require_not_finalized` and `require_auth` ordering intact.
- Document the approval entrypoint in the pause matrix.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-pause-gate-approvals`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/pause_controls.rs`](contracts/escrow/src/test/pause_controls.rs).
  - **Add documentation:** update the pause matrix under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) noting the gate.
  - Validate security: approvals cannot be staged during pause or emergency.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover paused, emergency, and normal approval paths.
- Include the full `cargo test` output in the PR description.

### Example commit message
`fix: gate approve_milestone_release behind pause and emergency controls`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Credit pending reputation consistently after a dispute resolves to Completed"
labels: type:security, area:reputation, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Credit pending reputation consistently after a dispute resolves to Completed

### Description
One `resolve_dispute` implementation in [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) increments `DataKey::PendingReputationCredits(freelancer)` when the resolution ends `Completed`, but the canonical `resolve_dispute` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) does **not**. Likewise normal `release_milestone` completion never grants the pending credit. As a result, whether a freelancer can later receive `issue_reputation` depends on which path completed the contract, and `issue_reputation` already panics with `InvalidState` when the pending credit is `<= 0`.

This issue makes pending-credit accrual on completion uniform across both the release and dispute-resolution paths.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Grant exactly one pending reputation credit to the freelancer whenever a contract transitions to `Completed`, regardless of path (final milestone release or dispute resolution).
- Ensure `issue_reputation` then succeeds and decrements the credit to zero.
- Avoid double-crediting if completion is reached once.
- Document the credit lifecycle from completion to `issue_reputation`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-reputation-credit-on-complete`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) (release completion) and the canonical `resolve_dispute`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/reputation.rs`](contracts/escrow/src/test/reputation.rs).
  - **Add documentation:** document the credit lifecycle under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the credit increment.
  - Validate security: no double credit, no orphaned credit.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover completion via release and via dispute, then `issue_reputation`.
- Include the full `cargo test` output in the PR description.

### Example commit message
`fix: grant pending reputation credit consistently on completion`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Validate comment length bounds (1..=200) in issue_reputation with explicit tests"
labels: type:test, area:reputation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Validate comment length bounds (1..=200) in issue_reputation with explicit tests

### Description
`issue_reputation` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) rejects an empty `comment` with `EmptyComment` and a comment longer than 200 bytes with `CommentTooLong`, bounding on-chain storage. These bounds currently have no dedicated tests at the boundary values, and the byte-vs-character distinction (`comment.len()` is byte length) is undocumented for multi-byte UTF-8.

This issue adds boundary tests at lengths 0, 1, 200, and 201 and clarifies that the bound is measured in bytes.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Test: length 0 panics with `EmptyComment`.
- Test: length 1 and length 200 succeed (assuming all other gates pass).
- Test: length 201 panics with `CommentTooLong`.
- Add a doc note that the 200 limit is byte length, so multi-byte characters count as more than one.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-reputation-comment-bounds`
- Implement changes
  - **Write code in:** no production change unless the byte-length doc requires a tweak in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/reputation.rs`](contracts/escrow/src/test/reputation.rs).
  - **Add documentation:** clarify the comment bound under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the bound.
  - Validate security: oversized comments cannot bloat storage.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover lengths 0, 1, 200, and 201.
- Include the full `cargo test` output in the PR description.

### Example commit message
`test: cover comment length bounds in issue_reputation`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Store the rating comment and expose it through a get_reputation_comment reader"
labels: type:feature, area:reputation, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Store the rating comment and expose it through a get_reputation_comment reader

### Description
`issue_reputation` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) accepts and length-validates a `comment: String` but then **discards it** — only the numeric `rating` lands in the `Reputation` record. The reviewer's written feedback, the most human-useful part of a reputation system, never reaches storage and cannot be read back.

This issue persists the comment per contract (or per reputation entry) and adds a reader so clients can display the feedback that justified a rating.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Persist the comment keyed by contract id (e.g. a new `DataKey::ReputationComment(u32)`), set once during `issue_reputation`.
- Add `get_reputation_comment(contract_id) -> Option<String>`.
- Keep the existing 200-byte bound; bump persistent TTL where the comment is stored.
- Document the new storage key and reader.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-store-reputation-comment`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) and [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) (new DataKey variant, append-only).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/reputation.rs`](contracts/escrow/src/test/reputation.rs).
  - **Add documentation:** describe the comment storage under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the reader.
  - Validate security: comment bound still enforced before storage.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover set-then-read and absent-comment reads.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat: persist and expose reputation comment with tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Cap set_protocol_fee_bps at 10_000 to mirror set_governed_params validation"
labels: type:security, area:governance, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Cap set_protocol_fee_bps at 10_000 to mirror set_governed_params validation

### Description
`set_governed_params` in [`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs) rejects a `protocol_fee_bps > 10_000` with `InvalidProtocolParameters`, but the standalone `set_protocol_fee_bps` in the same module performs **no upper-bound check** and will happily store an absurd value such as `50_000` (500%). At release time, `calculate_protocol_fee` would then compute a fee larger than the milestone amount, breaking accounting.

This issue applies the same `<= 10_000` ceiling to `set_protocol_fee_bps` so both setters are equally safe.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Reject `new_bps > 10_000` in `set_protocol_fee_bps` with `InvalidProtocolParameters` before persisting.
- Keep the existing event emission for valid updates.
- Add tests at `0`, `10_000` (allowed), and `10_001` (rejected).
- Document that basis points are bounded to 100%.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-fee-bps-upper-bound`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/protocol_fees.rs`](contracts/escrow/src/test/protocol_fees.rs).
  - **Add documentation:** note the 10_000 ceiling under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the bound.
  - Validate security: a fee can never exceed 100% of a milestone.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover boundary values 10_000 and 10_001.
- Include the full `cargo test` output in the PR description.

### Example commit message
`fix: bound set_protocol_fee_bps to 10_000 basis points`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add an arbiter-assignment entrypoint for contracts created without one"
labels: type:feature, area:dispute, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add an arbiter-assignment entrypoint for contracts created without one

### Description
`create_contract` in [`contracts/escrow/src/create_contract.rs`](contracts/escrow/src/create_contract.rs) only requires an arbiter for `ArbiterOnly`/`ClientAndArbiter` release modes; a `ClientOnly` or `MultiSig` contract can be created with `arbiter: None`. But `raise_dispute` panics with `ArbiterRequired` when no arbiter is assigned, so such contracts can never enter the dispute path even if both parties later want one.

This issue adds an entrypoint to assign an arbiter after creation (when none exists yet), unblocking disputes for contracts that started without one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `assign_arbiter(contract_id, caller, arbiter)` requiring authorization from both client and freelancer (or a documented mutual-consent scheme).
- Reject assigning an arbiter equal to the client or freelancer (`InvalidArbiter`).
- Only allow assignment when `contract.arbiter.is_none()` and status is pre-dispute (`Created`/`Funded`/`PartiallyFunded`).
- Emit an event and gate behind pause/emergency and not-finalized.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-assign-arbiter`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) (or a small helper in `dispute.rs`).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/dispute.rs`](contracts/escrow/src/test/dispute.rs).
  - **Add documentation:** describe arbiter assignment under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the entrypoint.
  - Validate security: cannot overwrite an existing arbiter or set an invalid one.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover assignment success, duplicate-arbiter rejection, and post-assignment dispute.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat: add assign_arbiter entrypoint for arbiter-less contracts`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Block release_milestone, deposit, and refund on a Disputed contract via tests"
labels: type:test, area:dispute, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Block release_milestone, deposit, and refund on a Disputed contract via tests

### Description
`raise_dispute` transitions a contract to `Disputed` precisely to freeze releases until an arbiter resolves it. `release_milestone` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) requires `status == Funded`, so a release on a disputed contract should fail — but there is no explicit regression test asserting this, and `refund_unreleased_milestones` *does* allow `Disputed`, while `deposit_funds` requires `Created`. The exact behavior during a dispute is therefore implicit and unguarded.

This issue pins down, with tests, exactly which money-flow operations are blocked or allowed while a contract is `Disputed`.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Test: `release_milestone` on a `Disputed` contract panics with `InvalidState`.
- Test: `deposit_funds` on a `Disputed` contract panics with `InvalidState`.
- Test: `refund_unreleased_milestones` behavior on `Disputed` matches the documented intent (currently allowed).
- Document the disputed-state operation matrix.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-disputed-state-operations`
- Implement changes
  - **Write code in:** no production change expected unless an inconsistency is found in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/dispute.rs`](contracts/escrow/src/test/dispute.rs).
  - **Add documentation:** add the disputed-state matrix under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on any helper.
  - Validate security: no value moves to the freelancer mid-dispute.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover release, deposit, and refund against a disputed contract.
- Include the full `cargo test` output in the PR description.

### Example commit message
`test: pin money-flow operations blocked during dispute`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Remove the duplicate raise_dispute and resolve_dispute definitions in dispute.rs"
labels: type:refactor, area:dispute, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Remove the duplicate raise_dispute and resolve_dispute definitions in dispute.rs

### Description
[`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) contains **three** `#[contractimpl] impl Escrow` blocks defining `raise_dispute` (and two defining `resolve_dispute`), plus a fourth canonical pair in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs). One of the `dispute.rs` variants even skips the arbiter and status guards entirely. Duplicate `contractimpl` methods cannot coexist, and the redundant copies have diverging authorization logic — a latent footgun where the weaker variant could shadow the safe one.

This issue collapses the dispute entrypoints down to a single authoritative implementation and deletes the rest, along with the duplicate `use`/import lines.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Keep exactly one `raise_dispute` and one `resolve_dispute` (the version with full arbiter/status/pause/not-finalized guards and accounting-invariant check).
- Delete the weaker duplicates and the duplicated `use soroban_sdk::{…}` imports at the top of `dispute.rs`.
- Keep `resolution_payouts` and `final_status_after_resolution` as the shared helpers.
- Ensure the surviving implementation increments pending reputation credit on completion (coordinate with the reputation-credit issue).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-dedupe-dispute-impls`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) and [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/dispute.rs`](contracts/escrow/src/test/dispute.rs).
  - **Add documentation:** note the single dispute entrypoint under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the surviving entrypoints.
  - Validate security: the weaker no-arbiter variant is gone.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover full guard coverage on the surviving entrypoints.
- Include the full `cargo test` output in the PR description.

### Example commit message
`refactor: collapse duplicate dispute entrypoints to one safe implementation`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Fix the corrupted get_mainnet_readiness_info body that writes a settlement token"
labels: type:security, area:readiness, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Fix the corrupted get_mainnet_readiness_info body that writes a settlement token

### Description
`get_mainnet_readiness_info` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) is documented as a read-only accessor returning the `ReadinessChecklist`, but its body has been corrupted: it instead writes a `DataKey::SettlementToken`, publishes a `settl_tok` event, references undeclared `token`/`admin` locals, and returns `true` — none of which match its signature returning `ReadinessChecklist`. The function cannot compile and the readiness checklist is unreadable.

This issue restores `get_mainnet_readiness_info` to a pure read that returns the stored (or default) `ReadinessChecklist`.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Replace the body with a read of `DataKey::ReadinessChecklist`, returning `unwrap_or_default()`.
- Remove the stray settlement-token write/event and undeclared locals from this function.
- Do not introduce a settlement-token feature here; that belongs in its own issue if desired.
- Keep the documented checklist-field semantics intact.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-fix-readiness-getter`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/mainnet_readiness.rs`](contracts/escrow/src/test/mainnet_readiness.rs).
  - **Add documentation:** confirm the read-only contract of the getter under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the getter.
  - Validate security: the getter performs no state mutation.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover default checklist and a checklist after initialize/emergency.
- Include the full `cargo test` output in the PR description.

### Example commit message
`fix: restore get_mainnet_readiness_info to a pure checklist read`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Remove the duplicate mod amount_validation and re-exported safe_add_amounts in lib.rs"
labels: type:refactor, area:build-hygiene, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Remove the duplicate mod amount_validation and re-exported safe_add_amounts in lib.rs

### Description
The top of [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) declares `mod amount_validation;` **twice**, re-exports `safe_add_amounts` from the module and then defines `safe_add_amounts` **again as a free function twice** in the same file, and pulls `safe_subtract_amounts` through both a `pub use` and a `pub(crate) use`. These duplicate definitions and imports cannot all coexist and obscure which `safe_add_amounts` callers actually resolve to.

This issue de-duplicates the module declarations, imports, and the `safe_add_amounts` definition so there is exactly one of each.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Declare `mod amount_validation;` exactly once.
- Keep a single canonical `safe_add_amounts` (prefer the one in [`contracts/escrow/src/amount_validation.rs`](contracts/escrow/src/amount_validation.rs)) and remove the duplicate free-function copies in `lib.rs`.
- Collapse the redundant `pub use` / `pub(crate) use` lines for `safe_add_amounts`/`safe_subtract_amounts`.
- Confirm all call sites still resolve to the surviving function.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-dedupe-amount-validation`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/security.rs`](contracts/escrow/src/test/security.rs) — overflow/underflow round-trips through the surviving helpers.
  - **Add documentation:** note the canonical location of the amount helpers.
  - Include NatSpec-style doc comments (`///`) on the helpers.
  - Validate security: checked-arithmetic behavior is unchanged.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover add/subtract overflow and underflow.
- Include the full `cargo test` output in the PR description.

### Example commit message
`refactor: de-duplicate amount_validation module and safe_add_amounts`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Collapse the duplicate resolve_emergency and DataKey definitions to single sources"
labels: type:refactor, area:build-hygiene, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Collapse the duplicate resolve_emergency and DataKey definitions to single sources

### Description
[`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) defines `resolve_emergency` **twice** in the same `impl` block (the first variant skips `require_initialized`, the second enforces it). Separately, [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) declares the `DataKey` enum **twice** (and `Contract`, `ReleaseAuthorization`, `MilestoneApprovals`, `DepositMode` are each declared twice too). Duplicate methods and `#[contracttype]` enums cannot compile, and the two `DataKey` copies omit the `SettlementToken`/`GovernanceAdmin` discrepancy noted elsewhere.

This issue keeps one `resolve_emergency` (the initialized-guarded variant) and one canonical `DataKey`/`Contract`/`ReleaseAuthorization`/`MilestoneApprovals`/`DepositMode` definition.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Keep the `resolve_emergency` that calls `require_initialized`, delete the other.
- Keep one definition each of `DataKey`, `Contract`, `ReleaseAuthorization`, `MilestoneApprovals`, and `DepositMode` in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs).
- Ensure the surviving `DataKey` includes every variant the code references (e.g. `SettlementToken` if used), keeping discriminants append-only.
- Re-run all references to confirm they resolve to the surviving types.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-dedupe-types-and-resolve-emergency`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) and [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/pause_controls.rs`](contracts/escrow/src/test/pause_controls.rs) and [`contracts/escrow/src/test/persistence.rs`](contracts/escrow/src/test/persistence.rs).
  - **Add documentation:** document the canonical type/key locations.
  - Include NatSpec-style doc comments (`///`) on the surviving definitions.
  - Validate security: emergency resolution still requires initialization and admin auth.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover resolve_emergency auth gating and a DataKey round-trip.
- Include the full `cargo test` output in the PR description.

### Example commit message
`refactor: collapse duplicate resolve_emergency and DataKey definitions`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Deduplicate the propose/accept client migration wrappers between lib.rs and migration.rs"
labels: type:refactor, area:migration, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Deduplicate the propose/accept client migration wrappers between lib.rs and migration.rs

### Description
[`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) defines `propose_client_migration`, `accept_client_migration`, `has_pending_client_migration`, and `get_pending_client_migration` **twice** — once delegating to `migration::*_impl` and once delegating to `Self::*_impl`. Only one set can survive, and the two delegate to different call shapes, so the build is ambiguous about which path the migration entrypoints actually use.

This issue keeps a single wrapper set per entrypoint, delegating consistently to the implementations in [`contracts/escrow/src/migration.rs`](contracts/escrow/src/migration.rs).

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Keep exactly one public wrapper for each of the four migration entrypoints.
- Delegate all four to the canonical `migration::*` functions (matching the existing `propose_client_migration_impl` signature taking `&env`).
- Remove the redundant `Self::*_impl` wrappers and any now-dead `use` lines.
- Confirm the migration tests still pass against the surviving wrappers.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-dedupe-migration-wrappers`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/client_migration.rs`](contracts/escrow/src/test/client_migration.rs).
  - **Add documentation:** note the single migration wrapper set.
  - Include NatSpec-style doc comments (`///`) on the surviving wrappers.
  - Validate security: migration auth (current client) is preserved.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover propose, accept, has-pending, and get-pending paths.
- Include the full `cargo test` output in the PR description.

### Example commit message
`refactor: keep a single client-migration wrapper set delegating to migration.rs`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Use the ttl::load_milestones and store_milestones helpers across deposit, release, and refund"
labels: type:refactor, area:storage-ttl, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Use the ttl::load_milestones and store_milestones helpers across deposit, release, and refund

### Description
[`contracts/escrow/src/ttl.rs`](contracts/escrow/src/ttl.rs) provides `load_milestones`/`store_milestones`/`milestone_storage_key` that read or write the milestone vector *and* bump its TTL in one call. Yet `release_milestone`, `refund_unreleased_milestones`, `deposit_funds`, and `submit_work_evidence` each re-build `Symbol::new(&env, "milestones")` and the `(DataKey::Contract(id), key)` tuple by hand, then call a separate `extend_milestone_ttl`. This repetition is exactly what the helpers exist to remove, and a missed TTL bump after one of these manual writes would risk silent milestone eviction.

This issue routes every milestone read/write through the `ttl` helpers.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Replace manual `Symbol::new(&env, "milestones")` + tuple-key reads/writes with `ttl::load_milestones`/`ttl::store_milestones`.
- Ensure each write path still bumps TTL (the helpers do this) and remove now-redundant `extend_milestone_ttl` calls.
- Keep behavior identical; this is a mechanical de-duplication.
- Add a regression test asserting milestone TTL is extended after a release.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-milestone-helpers`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) and [`contracts/escrow/src/deposit.rs`](contracts/escrow/src/deposit.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/persistence.rs`](contracts/escrow/src/test/persistence.rs).
  - **Add documentation:** note the canonical milestone access helpers.
  - Include NatSpec-style doc comments (`///`) on the helpers.
  - Validate security: no write path skips the TTL bump.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover read/write/TTL behavior across deposit, release, and refund.
- Include the full `cargo test` output in the PR description.

### Example commit message
`refactor: route milestone reads and writes through ttl helpers`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a get_accumulated_protocol_fees reader and withdraw guard test for the fee treasury"
labels: type:test, area:protocol-fees, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a get_accumulated_protocol_fees reader and withdraw guard test for the fee treasury

### Description
`release_milestone` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) accrues fees into `DataKey::AccumulatedProtocolFees`, and an earlier batch added withdrawal with `InsufficientAccumulatedFees`. But there is no public reader returning the current accumulated balance, and no test asserting accrual across multiple releases sums correctly before a withdrawal. Operators cannot observe the treasury balance on-chain.

This issue adds `get_accumulated_protocol_fees() -> i128` and tests that accrual over several releases matches the sum of per-milestone fees.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only `get_accumulated_protocol_fees` returning the stored balance (default `0`).
- Test: with a non-zero fee bps, releasing N milestones accrues exactly `sum(calculate_protocol_fee(amount_i, bps))`.
- Test: a withdrawal exceeding the balance panics with `InsufficientAccumulatedFees`.
- Document the reader alongside the existing fee withdrawal docs.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-accumulated-fees-reader`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/protocol_fees.rs`](contracts/escrow/src/test/protocol_fees.rs).
  - **Add documentation:** add the reader to the protocol-fee docs under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the reader.
  - Validate security: the reader performs no mutation.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover multi-release accrual and over-withdraw rejection.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat: add get_accumulated_protocol_fees reader with accrual tests`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Deduct accrued protocol fees from the freelancer's milestone payout amount"
labels: type:feature, area:protocol-fees, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Deduct accrued protocol fees from the freelancer's milestone payout amount

### Description
On `release_milestone` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs), the protocol fee is *accumulated* into `AccumulatedProtocolFees` and `contract.released_amount` is increased by the **full** `milestone.amount`. The freelancer is therefore credited the gross amount while the protocol also banks a fee, so `released_amount + accumulated_fees` can exceed `funded_amount` — the fee is effectively double-counted against the escrow balance.

This issue makes the fee come out of the released amount: the freelancer's net payout is `amount - fee`, the fee is accrued, and the accounting invariant `released + refunded + fees <= funded` holds.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Compute `fee = calculate_protocol_fee(amount, bps)`, accrue it, and credit the freelancer `amount - fee` (track net released separately or document that fees are part of `released_amount`).
- Add an accounting invariant assertion: `released_amount + refunded_amount + accumulated_fees_for_contract <= funded_amount`.
- Ensure `available_balance` checks account for the fee so a milestone cannot release more than the escrow holds.
- Add property tests over sequences of releases with a non-zero fee.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-fee-deducted-payout`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/protocol_fees.rs`](contracts/escrow/src/test/protocol_fees.rs) and [`contracts/escrow/src/test/resolution_payouts_prop.rs`](contracts/escrow/src/test/resolution_payouts_prop.rs).
  - **Add documentation:** clarify gross-vs-net payout under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the deduction.
  - Validate security: invariant holds across all release sequences.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover zero-fee, non-zero-fee, and multi-release accounting.
- Include the full `cargo test` output in the PR description.

### Example commit message
`fix: deduct protocol fee from milestone payout and preserve accounting invariant`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a contract-existence reader (contract_exists) to avoid panicking probe calls"
labels: type:feature, area:indexer-views, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a contract-existence reader (contract_exists) to avoid panicking probe calls

### Description
The only way to check whether a `contract_id` exists today is to call `get_contract`, which panics with `ContractNotFound` for unknown ids (per [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs)). Clients and indexers iterating an id range must catch panics or pre-know the range, which is awkward and wasteful.

This issue adds a cheap, non-panicking `contract_exists(contract_id) -> bool` so callers can probe ids safely.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `contract_exists` returning `env.storage().persistent().has(&DataKey::Contract(contract_id))`.
- Do not extend TTL on a pure existence probe (document this), so probing cannot be abused to keep entries alive.
- Add a companion `get_next_contract_id()` reader so indexers know the allocation high-water mark.
- Document both readers.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-existence-reader`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/contract_id_allocation.rs`](contracts/escrow/src/test/contract_id_allocation.rs).
  - **Add documentation:** add both readers to the entrypoint reference under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on both readers.
  - Validate security: existence probe does not mutate or extend TTL.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover existing id, missing id, and next-id high-water mark.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat: add contract_exists and get_next_contract_id readers`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Verify dispute resolution conserves funded_amount via the accounting-invariant guard"
labels: type:test, area:dispute, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Verify dispute resolution conserves funded_amount via the accounting-invariant guard

### Description
The full `resolve_dispute` in [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) asserts `released_amount + refunded_amount == funded_amount` after applying payouts, panicking with `AccountingInvariantViolated` otherwise. This is the core safety property of dispute resolution, but it is not exercised by a test that deliberately constructs a non-conserving `Split` or a contract with pre-existing partial releases.

This issue adds tests that prove the invariant guard fires when it must and passes when payouts conserve the balance, including disputes on contracts that already had some milestones released.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Test: a `Split(a, b)` whose sum equals the available balance succeeds and ends `Completed`/`Refunded` correctly.
- Test: a `Split` whose sum differs from available is rejected by `resolution_payouts` (`InvalidDisputeSplit`) before the invariant check.
- Test: dispute on a contract with prior partial releases — confirm `released + refunded == funded` after `FullRefund`, `FullPayout`, and `PartialRefund`.
- Assert the dispute-resolved event payload carries the client/freelancer payouts.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-dispute-conservation`
- Implement changes
  - **Write code in:** no production change expected unless a gap is found in [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/dispute.rs`](contracts/escrow/src/test/dispute.rs).
  - **Add documentation:** state the conservation invariant under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on any helper.
  - Validate security: no resolution can create or destroy value.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover all four resolutions over contracts with and without prior releases.
- Include the full `cargo test` output in the PR description.

### Example commit message
`test: prove dispute resolution conserves funded_amount`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a get_finalization_record round-trip test that captures milestone summaries"
labels: type:test, area:finalization, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a get_finalization_record round-trip test that captures milestone summaries

### Description
`finalize_contract_impl` in [`contracts/escrow/src/finalize.rs`](contracts/escrow/src/finalize.rs) snapshots the contract into a `FinalizationRecord` whose `summary` includes per-milestone `MilestoneSummary` rows, `total_amount`, `released_milestone_count`, and `refundable_balance` computed with checked subtraction. There is no test asserting the snapshot's milestone summaries and derived totals match the live contract at finalization time.

This issue adds a round-trip test: finalize a multi-milestone contract with mixed released/refunded milestones and assert every `summary` field is correct.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Finalize a `Completed` contract with 3 milestones (some released, one refunded) and read back the record.
- Assert `total_amount`, `released_milestone_count`, `refundable_balance`, and each `MilestoneSummary { index, amount, released, refunded }`.
- Assert `schema_version == CONTRACT_SUMMARY_SCHEMA_VERSION` and `reputation_issued` reflects state.
- Assert a second finalize attempt panics with `AlreadyFinalized`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-finalization-summary-roundtrip`
- Implement changes
  - **Write code in:** no production change expected unless a summary mismatch is found in [`contracts/escrow/src/finalize.rs`](contracts/escrow/src/finalize.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/persistence.rs`](contracts/escrow/src/test/persistence.rs) or a dedicated finalization test module.
  - **Add documentation:** confirm the summary schema under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on any helper.
  - Validate security: the record is immutable once written.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover the summary fields and the double-finalize rejection.
- Include the full `cargo test` output in the PR description.

### Example commit message
`test: round-trip finalization record milestone summaries`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Require initialization in cancel_contract and submit_work_evidence for a uniform safety rail"
labels: type:security, area:initialization, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Require initialization in cancel_contract and submit_work_evidence for a uniform safety rail

### Description
The `require_initialized` doc comment in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) claims it is called at the top of "every lifecycle entrypoint," yet `cancel_contract`, `submit_work_evidence`, `raise_dispute`, and `resolve_dispute` only call `require_not_paused`/`require_not_finalized` — never `require_initialized`. On an uninitialized contract there is no admin, so the pause and emergency rails these functions rely on can never have been armed, leaving a gap between the documented and actual safety model.

This issue adds the `require_initialized` gate to the lifecycle entrypoints that currently omit it, matching the stated invariant.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `Self::require_initialized(&env)` at the top of `cancel_contract`, `submit_work_evidence`, `raise_dispute`, and `resolve_dispute`.
- Return `NotInitialized` when called before `initialize`.
- Keep ordering: initialization check, then pause/emergency, then auth, then state.
- Update the entrypoint docs to list which functions enforce initialization.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-uniform-init-gate`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) and the dispute path in [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/security.rs`](contracts/escrow/src/test/security.rs).
  - **Add documentation:** update the initialization-required list under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the added gate.
  - Validate security: no lifecycle mutation runs on an uninitialized contract.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover each entrypoint called before initialize.
- Include the full `cargo test` output in the PR description.

### Example commit message
`fix: enforce require_initialized across all lifecycle entrypoints`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a CONTRACT_SUMMARY_SCHEMA_VERSION bump-and-migrate path for the indexer summary"
labels: type:enhancement, area:indexer-views, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a CONTRACT_SUMMARY_SCHEMA_VERSION bump-and-migrate path for the indexer summary

### Description
`CONTRACT_SUMMARY_SCHEMA_VERSION` in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) is pinned to `1` and is marked `#[allow(dead_code)]`, while `ContractSummary` is stamped with it in [`contracts/escrow/src/finalize.rs`](contracts/escrow/src/finalize.rs). There is no documented procedure or test for what happens when a field is added to `ContractSummary` — indexers have no signal to expect a v2 layout, and the version is never validated on read.

This issue establishes a forward-compatible versioning policy: document how to bump the version, add a reader that exposes the schema version, and test that old finalized records remain decodable.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a `get_contract_summary_schema_version() -> u32` reader and remove the now-unnecessary `dead_code` allow.
- Document the rule: additive field changes bump the version; consumers branch on `schema_version`.
- Add a test asserting a finalized record carries the current schema version.
- Note backward-compatibility expectations for already-written records.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-summary-schema-versioning`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) and [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/mainnet_readiness.rs`](contracts/escrow/src/test/mainnet_readiness.rs) or a summary test module.
  - **Add documentation:** publish the versioning policy under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the version constant and reader.
  - Validate security: version reads do not mutate state.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover the reader and the stamped version on a finalized record.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat: expose and document ContractSummary schema versioning`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Document the protocol fee model from accrual through treasury withdrawal"
labels: type:docs, area:protocol-fees, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the protocol fee model from accrual through treasury withdrawal

### Description
Protocol fees touch several files — `set_protocol_fee_bps`/`set_governed_params` set the rate, `calculate_protocol_fee` computes it, `release_milestone` accrues it into `AccumulatedProtocolFees`, and an admin withdrawal drains it — yet there is no single document that explains the basis-point model, the rounding rule, where fees are stored, and who can withdraw. New contributors must reverse-engineer the flow from [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) and [`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs).

This issue writes an authoritative protocol-fee document covering the full lifecycle.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Document: the basis-point unit (10_000 = 100%), the `amount * bps / 10_000` formula, floor rounding, and the 10_000 cap.
- Document where fees accrue (`AccumulatedProtocolFees`), when (on each release), and the admin-only withdrawal with `InsufficientAccumulatedFees`.
- Include a worked numeric example and a sequence diagram from release to withdrawal.
- Cross-link the relevant entrypoints.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-protocol-fee-model`
- Implement changes
  - **Write code in:** no production change; documentation only (reference [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs)).
  - **Write comprehensive tests in:** not applicable; if a doctest-style example is added, place it in [`contracts/escrow/src/test/protocol_fees.rs`](contracts/escrow/src/test/protocol_fees.rs).
  - **Add documentation:** create `docs/escrow/protocol-fees.md` under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) cross-referencing the doc from the fee functions.
  - Validate security: document the admin-only withdrawal authorization.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Verify any embedded example matches actual behavior.
- Include the full `cargo test` output in the PR description.

### Example commit message
`docs: document the protocol fee model from accrual to withdrawal`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Document the two-step governance admin rotation flow and its timelock"
labels: type:docs, area:governance, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the two-step governance admin rotation flow and its timelock

### Description
Admin rotation spans `propose_governance_admin` → wait `ADMIN_ROTATION_MIN_DELAY_LEDGERS` → `accept_governance_admin` in [`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs), guarded by `PendingAdminProposal` and a `TimelockNotElapsed` error, and surfaced through `get_pending_governance_admin`. None of this is documented end-to-end: contributors do not know the ~2-day delay, who must authorize each step, or how to read pending state.

This issue documents the full admin-rotation runbook with the authorization, timelock, and observability details.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Document each step: current admin proposes (requires current-admin auth), the timelock window in ledgers and approximate days, and proposed-admin acceptance (requires proposed-admin auth).
- Document the `TimelockNotElapsed` rejection and how to read `get_pending_governance_admin` and the anchor ledger.
- Include the emitted `admin/proposed` and `admin/accepted` event payloads.
- Provide a CLI-style example sequence.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-admin-rotation-runbook`
- Implement changes
  - **Write code in:** no production change; documentation only (reference [`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs)).
  - **Write comprehensive tests in:** not applicable; cross-link to the timelock tests in [`contracts/escrow/src/test/admin_auth_helper.rs`](contracts/escrow/src/test/admin_auth_helper.rs).
  - **Add documentation:** create `docs/escrow/admin-rotation.md` under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) cross-referencing the runbook.
  - Validate security: document that both steps require distinct authorizations.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Verify the documented steps match the implementation.
- Include the full `cargo test` output in the PR description.

### Example commit message
`docs: document two-step admin rotation flow and timelock`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Document the milestone approval and release flow including MultiSig semantics"
labels: type:docs, area:release, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the milestone approval and release flow including MultiSig semantics

### Description
Releasing a milestone requires approvals recorded in temporary storage (`approve_milestone` in [`contracts/escrow/src/approvals.rs`](contracts/escrow/src/approvals.rs)) that satisfy the contract's `ReleaseAuthorization` mode, then a `release_milestone` call by an authorized party (`lib.rs`). The MultiSig rule is especially subtle: both client and freelancer must approve, but only one of them releases, and approvals expire by TTL and are cleared after release. This flow is scattered across two files and not documented as a whole.

This issue documents the approve → check → release → clear flow per authorization mode, including TTL expiry and the fail-closed behavior.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Document each `ReleaseAuthorization` mode's required approvers and who may release.
- Document approval TTL (`PENDING_APPROVAL_TTL_LEDGERS`), the fail-closed expiry behavior, and that approvals are cleared on release.
- Include a state diagram of approve → check_approvals → release_milestone → clear_approvals.
- Cross-link `approve_milestone_release`, `get_milestone_approvals`, and `release_milestone`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-approval-release-flow`
- Implement changes
  - **Write code in:** no production change; documentation only (reference [`contracts/escrow/src/approvals.rs`](contracts/escrow/src/approvals.rs)).
  - **Write comprehensive tests in:** not applicable; cross-link existing tests in [`contracts/escrow/src/test/release.rs`](contracts/escrow/src/test/release.rs).
  - **Add documentation:** create `docs/escrow/approvals-and-release.md` under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) cross-referencing the doc.
  - Validate security: document the fail-closed expiry guarantee.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Verify documented modes match the code.
- Include the full `cargo test` output in the PR description.

### Example commit message
`docs: document milestone approval and release flow with MultiSig semantics`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add proptest coverage for deposit_funds promotion to Funded across deposit splits"
labels: type:test, area:deposit, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add proptest coverage for deposit_funds promotion to Funded across deposit splits

### Description
`deposit_funds` in [`contracts/escrow/src/deposit.rs`](contracts/escrow/src/deposit.rs) accumulates deposits and promotes the contract from `Created` to `Funded` once `funded_amount >= sum(milestone.amount)`. The promotion boundary — across many different deposit orderings and amounts that sum to (or overshoot) the milestone total — is exactly the kind of property worth fuzzing, but the existing proptest suite does not target the deposit promotion threshold.

This issue adds property tests that deposit randomized split amounts and assert the contract is promoted to `Funded` precisely when (and only when) the cumulative deposits cover the milestone total.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Generate random milestone vectors and random deposit sequences (positive amounts) summing to varying totals.
- Property: status is `Funded` iff cumulative `funded_amount >= milestone_total`; otherwise it stays `Created`.
- Property: `funded_amount` equals the cumulative sum of accepted deposits; no overshoot is lost.
- Reuse the existing proptest harness style in [`contracts/escrow/src/proptest.rs`](contracts/escrow/src/proptest.rs).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-deposit-promotion-proptest`
- Implement changes
  - **Write code in:** no production change expected unless a promotion edge is found in [`contracts/escrow/src/deposit.rs`](contracts/escrow/src/deposit.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/proptest.rs`](contracts/escrow/src/proptest.rs) or [`contracts/escrow/src/test/deposit.rs`](contracts/escrow/src/test/deposit.rs).
  - **Add documentation:** note the promotion property under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the property helpers.
  - Validate security: no deposit ordering yields premature promotion.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover under-funded, exact, and overshoot deposit sequences.
- Include the full `cargo test` output in the PR description.

### Example commit message
`test: proptest deposit_funds promotion threshold across splits`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Emit a status-change event from cancel_contract, deposit promotion, and completion"
labels: type:enhancement, area:events, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Emit a status-change event from cancel_contract, deposit promotion, and completion

### Description
`create_contract` emits a `created` event and the dispute paths emit dispute events, but several important status transitions emit nothing: `deposit_funds` promoting a contract to `Funded`, `release_milestone` transitioning to `Completed`, and `cancel_contract` transitioning to `Cancelled` (per [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) and [`contracts/escrow/src/deposit.rs`](contracts/escrow/src/deposit.rs)). Indexers therefore cannot reconstruct the full lifecycle from events alone.

This issue adds a uniform `status_changed` event (with old and new status) emitted at each of these transitions.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a `status_changed` event carrying `(contract_id, old_status, new_status, timestamp)` whenever the contract status changes in `deposit_funds`, `release_milestone`, and `cancel_contract`.
- Use a single consistent topic so indexers can subscribe once.
- Avoid emitting when status does not actually change (e.g. a deposit that does not reach the funded threshold).
- Add tests asserting the event fires with the correct old/new pair.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-status-changed-events`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) and [`contracts/escrow/src/deposit.rs`](contracts/escrow/src/deposit.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/release.rs`](contracts/escrow/src/test/release.rs) and [`contracts/escrow/src/test/deposit.rs`](contracts/escrow/src/test/deposit.rs).
  - **Add documentation:** add the event to the event catalog under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the emitted event.
  - Validate security: events do not alter authorization or accounting.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover promotion, completion, and cancellation transitions.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat: emit status_changed events at promotion, completion, and cancellation`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add a refund event and tests for refund_unreleased_milestones status outcomes"
labels: type:test, area:refund, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a refund event and tests for refund_unreleased_milestones status outcomes

### Description
`refund_unreleased_milestones` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) marks milestones refunded, increments `refunded_amount`, and computes a final status: `Refunded` if all milestones are refunded, or `Completed` if some were released and the rest refunded. It emits **no event**, and the two status outcomes plus the duplicate-index and already-released guards lack dedicated assertions.

This issue emits a `refunded` event and adds tests pinning the `Refunded`-vs-`Completed` outcome and the rejection guards.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a `refunded` event carrying `(contract_id, total_refund_amount, new_status, timestamp)`.
- Test: refunding all milestones ends `Refunded`; refunding the remainder after some releases ends `Completed`.
- Test: duplicate index in the request panics with `DuplicateMilestoneInRefund`; an already-released milestone panics with `AlreadyReleased`.
- Test: refund exceeding available balance panics with `InsufficientFunds`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-refund-events-and-outcomes`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the event emission.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/deposit.rs`](contracts/escrow/src/test/deposit.rs) or a dedicated refund test module.
  - **Add documentation:** add the refund event to the event catalog under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the event.
  - Validate security: refund cannot exceed the available balance.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover both status outcomes and every rejection guard.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat: emit refunded event and test refund status outcomes`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Add tests asserting finalized contracts reject deposit, release, refund, and cancel"
labels: type:test, area:finalization, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests asserting finalized contracts reject deposit, release, refund, and cancel

### Description
`require_not_finalized` in [`contracts/escrow/src/finalize.rs`](contracts/escrow/src/finalize.rs) is called from `release_milestone`, `refund_unreleased_milestones`, `cancel_contract`, `approve_milestone_release`, `submit_work_evidence`, `raise_dispute`, and `resolve_dispute` to lock a contract after `finalize_contract` writes its close record. There is no single test suite that finalizes a contract and then verifies *every* mutating entrypoint rejects with `AlreadyFinalized`.

This issue adds a comprehensive finalized-lock test matrix covering all guarded entrypoints.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Finalize a `Completed` contract, then assert each mutating entrypoint panics with `AlreadyFinalized`: `release_milestone`, `refund_unreleased_milestones`, `cancel_contract`, `approve_milestone_release`, `submit_work_evidence`, `raise_dispute`, `resolve_dispute`.
- Assert read-only entrypoints (`get_contract`, `get_finalization_record`) still succeed after finalization.
- Assert a second `finalize_contract` also rejects with `AlreadyFinalized`.
- Keep the test table-driven so new mutating entrypoints are easy to add.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-finalized-lock-matrix`
- Implement changes
  - **Write code in:** no production change expected unless a guard is missing in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/security.rs`](contracts/escrow/src/test/security.rs).
  - **Add documentation:** list the finalized-lock guarantees under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on any helper.
  - Validate security: no mutation is possible after finalization.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover every guarded mutating entrypoint post-finalization.
- Include the full `cargo test` output in the PR description.

### Example commit message
`test: assert finalized contracts reject all mutating entrypoints`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
++++++
---
type: Feature
title: "Reject a Split dispute resolution whose components are individually within but jointly exceed balance"
labels: type:security, area:dispute, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Reject a Split dispute resolution whose components are individually within but jointly exceed balance

### Description
`resolution_payouts` in [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) handles `DisputeResolution::Split(client, freelancer)` by rejecting negative components and requiring `client + freelancer == available`. The `safe_add_amounts` guard catches overflow, but the boundary where each component is a large positive value near `i128::MAX` such that their sum overflows — and must be rejected as `PotentialOverflow` rather than wrapping into a value that coincidentally equals `available` — is not directly tested.

This issue adds targeted tests proving the `Split` validation rejects overflow-prone and mismatched component pairs, and accepts only exact, conserving splits.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Test: `Split` with a negative component returns `InvalidDisputeSplit`.
- Test: `Split` whose components sum to less/more than `available` returns `InvalidDisputeSplit`.
- Test: `Split` components near `i128::MAX` that would overflow return `PotentialOverflow`, not a wrapped match.
- Test: an exact, conserving `Split` succeeds and updates `released_amount`/`refunded_amount` correctly.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-split-overflow-guard`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) only if the overflow ordering needs hardening.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/dispute.rs`](contracts/escrow/src/test/dispute.rs) and [`contracts/escrow/src/test/resolution_payouts_prop.rs`](contracts/escrow/src/test/resolution_payouts_prop.rs).
  - **Add documentation:** note the Split validation order under [`docs/escrow`](docs/escrow).
  - Include NatSpec-style doc comments (`///`) on the Split branch.
  - Validate security: no overflow can masquerade as a valid split.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover negative, mismatched, overflow, and exact splits.
- Include the full `cargo test` output in the PR description.

### Example commit message
`test: harden Split dispute validation against overflow and mismatch`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.