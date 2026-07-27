---
type: Feature
title: "Add a read view returning a contract's completed vs total milestone counts"
labels: type:feature, area:milestones, stack:rust, stack:soroban, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose milestone progress in one call

### Description
There is no O(1) way to read how many of a contract's milestones are complete. This issue adds a `get_milestone_progress(contract_id)` view returning `(completed, total)`.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `get_milestone_progress` returning completed and total counts for a contract.
- Read-only; return `(0,0)` for an unknown contract without panicking.
- Reuse stored counters where available rather than iterating if a counter exists.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/milestones-03-progress-view`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/`](contracts/escrow/src/test/) — none complete, some complete, all complete, unknown contract.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: unknown contract, zero milestones, all complete.
- Include the full test output in the PR description.

### Example commit message
`feat(milestones): add get_milestone_progress view`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add tests for the arbiter assignment and reassignment authorization"
labels: type:test, area:disputes, stack:rust, stack:soroban, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover arbiter assignment auth

### Description
Assigning and reassigning the arbiter has authorization rules that are under-tested. This issue adds focused tests.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests asserting only the authorized party may set/reassign the arbiter, that reassignment is rejected in disallowed states, and the typed error codes match.
- Use the test-utils auth helpers.
- Do not change logic unless a defect is found (note it).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/disputes-02-arbiter-auth`
- Implement changes
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/`](contracts/escrow/src/test/).
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: non-authorized setter, reassign after dispute resolved, same-arbiter no-op.
- Include the full test output in the PR description.

### Example commit message
`test(disputes): cover arbiter assignment auth`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Convert remaining reputation-path panics to typed ContractError codes"
labels: type:refactor, area:reputation, stack:rust, stack:soroban, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type the reputation failures

### Description
Some reputation entrypoints still panic with strings. This issue migrates them to typed `ContractError` codes for stable, testable failures.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Map each reputation-path panic to an existing or new `ContractError` discriminant (no duplicates).
- Preserve the failure conditions exactly.
- Update tests that matched panic strings to assert the typed code.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/reputation-01-typed-errors`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/`](contracts/escrow/src/test/) — one assertion per typed rejection.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: invalid rating, duplicate attestation, unknown contract.
- Include the full test output in the PR description.

### Example commit message
`refactor(reputation): type the reputation failures`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Extract the repeated party-authorization preamble into a require_party helper"
labels: type:refactor, area:auth, stack:rust, stack:soroban, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate party auth checks

### Description
Multiple entrypoints repeat the same 'is caller a party to this contract' check. This issue extracts a `require_party` helper mirroring the existing require_active_contract pattern.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a private `require_party(contract, caller)` returning the party role or the typed error, and route entrypoints through it.
- Behaviour unchanged; same rejections and codes.
- No ABI change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/auth-01-require-party`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/`](contracts/escrow/src/test/) — party accepted, non-party rejected, unknown contract rejected.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: client vs freelancer, stranger rejected.
- Include the full test output in the PR description.

### Example commit message
`refactor(auth): extract require_party helper`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Emit a milestone-approved event carrying the released amount"
labels: type:feature, area:events, stack:rust, stack:soroban, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Emit an event on milestone approval

### Description
Milestone approval releases funds but emits no dedicated event, forcing indexers to infer it. This issue adds a `ms_appr` event.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a milestone-approval event with a `symbol_short!` topic (<= 9 chars) carrying the contract id, milestone index, and released amount.
- Do not change fund movement; ensure no topic collision.
- Capture events in tests immediately after the approving call (buffer holds only the latest invocation).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/events-02-milestone-approved`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/`](contracts/escrow/src/test/) — assert topic and payload.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: approval of the final milestone, no collision with existing topics.
- Include the full test output in the PR description.

### Example commit message
`feat(events): emit milestone-approved event`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Document the settlement and payout ordering guarantees"
labels: type:docs, area:settlement, stack:rust, stack:soroban, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document settlement ordering

### Description
The order in which funds, fees, and any arbiter award are paid at settlement is not documented, making audits harder. This issue documents the guarantees.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/settlement.md` describing the payout order, rounding, and how disputes/holds alter it.
- Cross-reference the settlement entrypoints with a worked numeric example.
- Keep it accurate — read the settlement path first.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/settlement-01-ordering`
- Implement changes
  - **Add documentation:** create `docs/settlement.md`.
- Test and commit

### Test and commit
- Run `cargo fmt` and `cargo test`.
- Cover edge cases: n/a — verify the example against a unit assertion.
- Include the full test output in the PR description.

### Example commit message
`docs(settlement): document payout ordering`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
