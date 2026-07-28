---
type: Feature
title: "Add a batch variant of the contracts entrypoint"
labels: type:feature, area:contracts, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Batch contracts

### Description
Callers must invoke contracts once per item, wasting fees. This issue adds a bounded batch entrypoint.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a batch contracts entrypoint processing a bounded vec atomically (all-or-nothing) with the same per-item checks.
- Reject over-limit batches with a typed error.
- Cover batch success, partial-invalid rejection, and over-limit.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-61-batch`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: batch ok, one invalid rolls back, over-limit rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(contracts): add batch entrypoint`

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
title: "Add authorization negative-path tests for contracts"
labels: type:test, area:contracts, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Auth-test contracts

### Description
contracts's authorization rejections aren't fully tested. This issue adds negative-path coverage.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests asserting contracts rejects unauthorized callers with the typed error across each guarded entrypoint.
- Cover admin-only and owner-only paths.
- No behaviour change unless a gap is found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-61-authneg`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: non-admin rejected, non-owner rejected.
- Include the full test output in the PR description.

### Example commit message
`test(contracts): cover auth negative paths`

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
title: "Extract contracts storage keys into a keys module"
labels: type:refactor, area:contracts, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Centralize contracts keys

### Description
contracts constructs storage keys inline, risking drift. This issue centralizes them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Move contracts storage-key construction into a single keys module and reference it everywhere.
- Identical key layout; no migration needed.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-61-keys`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same keys, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(contracts): centralize storage keys`

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
title: "Emit an event on contracts state changes"
labels: type:feature, area:contracts, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Event on contracts

### Description
contracts state changes are silent on-chain. This issue emits an event so indexers can react.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a documented event whenever contracts state changes, with the relevant fields.
- No duplicate emissions.
- Cover topic and payload in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-62-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: event emitted once, payload fields correct.
- Include the full test output in the PR description.

### Example commit message
`feat(contracts): emit state-change event`

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
title: "Add an invariants note for contracts"
labels: type:docs, area:contracts, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document contracts invariants

### Description
contracts's invariants (what must always hold) are undocumented. This issue records them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/contracts-invariants.md` listing the contracts invariants and where each is enforced.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-61-invariants`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(contracts): document invariants`

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
title: "Add a batch variant of the milestones entrypoint"
labels: type:feature, area:milestones, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Batch milestones

### Description
Callers must invoke milestones once per item, wasting fees. This issue adds a bounded batch entrypoint.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a batch milestones entrypoint processing a bounded vec atomically (all-or-nothing) with the same per-item checks.
- Reject over-limit batches with a typed error.
- Cover batch success, partial-invalid rejection, and over-limit.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/milestones-61-batch`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: batch ok, one invalid rolls back, over-limit rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(milestones): add batch entrypoint`

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
title: "Add authorization negative-path tests for milestones"
labels: type:test, area:milestones, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Auth-test milestones

### Description
milestones's authorization rejections aren't fully tested. This issue adds negative-path coverage.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests asserting milestones rejects unauthorized callers with the typed error across each guarded entrypoint.
- Cover admin-only and owner-only paths.
- No behaviour change unless a gap is found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/milestones-61-authneg`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: non-admin rejected, non-owner rejected.
- Include the full test output in the PR description.

### Example commit message
`test(milestones): cover auth negative paths`

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
title: "Extract milestones storage keys into a keys module"
labels: type:refactor, area:milestones, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Centralize milestones keys

### Description
milestones constructs storage keys inline, risking drift. This issue centralizes them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Move milestones storage-key construction into a single keys module and reference it everywhere.
- Identical key layout; no migration needed.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/milestones-61-keys`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same keys, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(milestones): centralize storage keys`

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
title: "Emit an event on milestones state changes"
labels: type:feature, area:milestones, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Event on milestones

### Description
milestones state changes are silent on-chain. This issue emits an event so indexers can react.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a documented event whenever milestones state changes, with the relevant fields.
- No duplicate emissions.
- Cover topic and payload in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/milestones-62-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: event emitted once, payload fields correct.
- Include the full test output in the PR description.

### Example commit message
`feat(milestones): emit state-change event`

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
title: "Add an invariants note for milestones"
labels: type:docs, area:milestones, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document milestones invariants

### Description
milestones's invariants (what must always hold) are undocumented. This issue records them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/milestones-invariants.md` listing the milestones invariants and where each is enforced.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/milestones-61-invariants`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(milestones): document invariants`

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
title: "Add a batch variant of the reputation entrypoint"
labels: type:feature, area:reputation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Batch reputation

### Description
Callers must invoke reputation once per item, wasting fees. This issue adds a bounded batch entrypoint.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a batch reputation entrypoint processing a bounded vec atomically (all-or-nothing) with the same per-item checks.
- Reject over-limit batches with a typed error.
- Cover batch success, partial-invalid rejection, and over-limit.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/reputation-61-batch`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: batch ok, one invalid rolls back, over-limit rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(reputation): add batch entrypoint`

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
title: "Add authorization negative-path tests for reputation"
labels: type:test, area:reputation, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Auth-test reputation

### Description
reputation's authorization rejections aren't fully tested. This issue adds negative-path coverage.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests asserting reputation rejects unauthorized callers with the typed error across each guarded entrypoint.
- Cover admin-only and owner-only paths.
- No behaviour change unless a gap is found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/reputation-61-authneg`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: non-admin rejected, non-owner rejected.
- Include the full test output in the PR description.

### Example commit message
`test(reputation): cover auth negative paths`

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
title: "Extract reputation storage keys into a keys module"
labels: type:refactor, area:reputation, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Centralize reputation keys

### Description
reputation constructs storage keys inline, risking drift. This issue centralizes them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Move reputation storage-key construction into a single keys module and reference it everywhere.
- Identical key layout; no migration needed.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/reputation-61-keys`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same keys, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(reputation): centralize storage keys`

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
title: "Emit an event on reputation state changes"
labels: type:feature, area:reputation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Event on reputation

### Description
reputation state changes are silent on-chain. This issue emits an event so indexers can react.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a documented event whenever reputation state changes, with the relevant fields.
- No duplicate emissions.
- Cover topic and payload in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/reputation-62-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: event emitted once, payload fields correct.
- Include the full test output in the PR description.

### Example commit message
`feat(reputation): emit state-change event`

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
title: "Add an invariants note for reputation"
labels: type:docs, area:reputation, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document reputation invariants

### Description
reputation's invariants (what must always hold) are undocumented. This issue records them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/reputation-invariants.md` listing the reputation invariants and where each is enforced.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/reputation-61-invariants`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(reputation): document invariants`

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
title: "Add a batch variant of the disputes entrypoint"
labels: type:feature, area:disputes, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Batch disputes

### Description
Callers must invoke disputes once per item, wasting fees. This issue adds a bounded batch entrypoint.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a batch disputes entrypoint processing a bounded vec atomically (all-or-nothing) with the same per-item checks.
- Reject over-limit batches with a typed error.
- Cover batch success, partial-invalid rejection, and over-limit.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/disputes-61-batch`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: batch ok, one invalid rolls back, over-limit rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(disputes): add batch entrypoint`

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
title: "Add authorization negative-path tests for disputes"
labels: type:test, area:disputes, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Auth-test disputes

### Description
disputes's authorization rejections aren't fully tested. This issue adds negative-path coverage.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests asserting disputes rejects unauthorized callers with the typed error across each guarded entrypoint.
- Cover admin-only and owner-only paths.
- No behaviour change unless a gap is found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/disputes-61-authneg`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: non-admin rejected, non-owner rejected.
- Include the full test output in the PR description.

### Example commit message
`test(disputes): cover auth negative paths`

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
title: "Extract disputes storage keys into a keys module"
labels: type:refactor, area:disputes, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Centralize disputes keys

### Description
disputes constructs storage keys inline, risking drift. This issue centralizes them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Move disputes storage-key construction into a single keys module and reference it everywhere.
- Identical key layout; no migration needed.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/disputes-61-keys`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same keys, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(disputes): centralize storage keys`

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
title: "Emit an event on disputes state changes"
labels: type:feature, area:disputes, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Event on disputes

### Description
disputes state changes are silent on-chain. This issue emits an event so indexers can react.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a documented event whenever disputes state changes, with the relevant fields.
- No duplicate emissions.
- Cover topic and payload in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/disputes-62-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: event emitted once, payload fields correct.
- Include the full test output in the PR description.

### Example commit message
`feat(disputes): emit state-change event`

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
title: "Add an invariants note for disputes"
labels: type:docs, area:disputes, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document disputes invariants

### Description
disputes's invariants (what must always hold) are undocumented. This issue records them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/disputes-invariants.md` listing the disputes invariants and where each is enforced.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/disputes-61-invariants`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(disputes): document invariants`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
