---
type: Feature
title: "Add a paginated view enumerating a contract's milestones with their status"
labels: type:feature, area:milestones, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose a contract's milestones through a bounded, paginated read view

### Description
The escrow contract in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) tracks milestones per contract, but there is no read-only way to page through them for a UI. This issue adds a `get_milestones_page(contract_id, start, limit)` view returning `(index, status, amount)` tuples using the existing start/limit pagination bounds.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `get_milestones_page` to [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) returning a `Vec` of a small `MilestoneEntry` struct.
- Reuse the shared start/limit bounds logic; do not re-implement clamping. Return an empty `Vec` for an unknown or empty contract rather than panicking.
- Read-only: no storage writes; keep the per-call length capped by the pagination ceiling.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/milestones-01-paginated-view`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/`](contracts/escrow/src/test/) — empty, single page, continuation, ceiling clamp.
  - Add rustdoc describing the pagination contract.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: unknown contract, exact-page boundary, over-limit clamp.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat(milestones): add paginated get_milestones_page view`

### Guidelines
- **Minimum 95 percent test coverage** for the impacted module.
- Clear, reviewer-focused rustdoc.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Convert remaining milestone-approval panic strings to typed ContractError codes"
labels: type:refactor, area:errors, stack:rust, stack:soroban, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Replace milestone-approval panics with typed errors

### Description
Some milestone-approval paths in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) still `panic!` with string messages instead of returning a typed `ContractError`. Typed errors give callers a stable numeric code and are testable with `assert_contract_error`. This issue migrates the remaining approval-path panics.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Identify each `panic!`/string-based failure in the milestone-approval flow and map it to an existing or new `ContractError` discriminant (no duplicate discriminants).
- Preserve the failure conditions exactly; only the error representation changes.
- Update any tests that matched on panic strings to assert the typed code.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/errors-01-milestone-typed`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/`](contracts/escrow/src/test/) — assert the exact typed code per rejection.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Confirm discriminants are unique across the `ContractError` enum.
- Include the full `cargo test` output in the PR description.

### Example commit message
`refactor(errors): type the milestone-approval failures`

### Guidelines
- **Minimum 95 percent test coverage** for the impacted module.
- Clear, reviewer-focused rustdoc.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add tests for the dispute-raise and resolve lifecycle across authorized and unauthorized callers"
labels: type:test, area:disputes, stack:rust, stack:soroban, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover the dispute lifecycle and its authorization rules

### Description
The dispute flow (raise, then resolve by the arbiter) in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) needs stronger test coverage around who may act and in what state. This issue adds tests for the full lifecycle and the auth boundaries.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests under [`contracts/escrow/src/test/`](contracts/escrow/src/test/) asserting: only a party to the contract may raise a dispute; only the designated arbiter may resolve it; resolving moves funds/state correctly; raising in a terminal state is rejected with the typed error.
- Use the test-utils auth helpers; assert exact typed `ContractError` codes for each rejection.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/disputes-01-lifecycle`
- Implement changes
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/`](contracts/escrow/src/test/).
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: non-party raise, non-arbiter resolve, double-resolve, resolve-after-settle.
- Include the full `cargo test` output in the PR description.

### Example commit message
`test(disputes): cover raise/resolve lifecycle and auth`

### Guidelines
- **Minimum 95 percent test coverage** for the impacted module.
- Clear, reviewer-focused test names.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Extract the repeated milestone bounds-and-existence check into a require_milestone helper"
labels: type:refactor, area:milestones, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate milestone lookup and bounds checking

### Description
Multiple milestone entrypoints in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) repeat the same "does this milestone index exist and is it in range" preamble. This issue extracts a `require_milestone(contract_id, index)` helper returning the milestone or the typed error, mirroring the existing `require_active_contract` pattern.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a private `require_milestone` helper and route every milestone entrypoint through it.
- Behaviour unchanged: same out-of-range/not-found rejections with the same typed `ContractError`.
- No ABI change beyond the internal refactor.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/milestones-02-require-helper`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/`](contracts/escrow/src/test/) — in-range ok, out-of-range rejected, unknown contract rejected.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Confirm no behaviour change against the existing milestone tests.
- Include the full `cargo test` output in the PR description.

### Example commit message
`refactor(milestones): extract require_milestone helper`

### Guidelines
- **Minimum 95 percent test coverage** for the impacted module.
- Clear, reviewer-focused rustdoc.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Document the reputation scoring model and how completed contracts update a score"
labels: type:docs, area:reputation, stack:rust, stack:soroban, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document the on-chain reputation scoring model

### Description
The contract updates a participant's reputation as contracts complete, but the scoring rules and their bounds are undocumented. This issue adds a focused doc so contributors and reviewers can reason about score changes.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/reputation.md` describing: what events change a score, the rating range, how a rating aggregates into the stored score, and any clamping.
- Cross-reference the reputation entrypoints in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) with a worked example.
- Keep it accurate to the current code — read the scoring logic first.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/reputation-01-scoring-model`
- Implement changes
  - **Add documentation:** create `docs/reputation.md`.
- Test and commit

### Test and commit
- Run `cargo fmt` and `cargo test` to confirm nothing else drifted.
- Validate the worked example against a quick unit assertion if practical.
- Note in the PR how you verified the numbers.

### Example commit message
`docs(reputation): document the scoring model`

### Guidelines
- Clear, reviewer-focused documentation with a worked example.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Emit a dedicated event when a contract is fully settled recording the final payout split"
labels: type:feature, area:events, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Emit a settlement event carrying the final payout split

### Description
When a contract fully settles there is no single event that records the final payout split (freelancer amount, any fees, arbiter award), forcing indexers to reconstruct it. This issue adds a dedicated `settled` event in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a `settled` event on the final settlement path with a `symbol_short!` topic (≤ 9 chars) and a data tuple carrying the split amounts and the contract id.
- Do not change fund movement; only add the event. Ensure the topic symbol does not collide with an existing event.
- Capture events in tests immediately after the settling call (the event buffer holds only the latest invocation).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/events-01-settled`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/`](contracts/escrow/src/test/) — assert the event topic and the payload split values.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: settlement with and without an arbiter award; verify no topic collision.
- Include the full `cargo test` output in the PR description.

### Example commit message
`feat(events): emit a settled event with the payout split`

### Guidelines
- **Minimum 95 percent test coverage** for the impacted module.
- Clear, reviewer-focused rustdoc.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
