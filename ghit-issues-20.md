---
type: Feature
title: "Add a read-only view exposing the current contracts state"
labels: type:feature, area:contracts, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose contracts through a read view

### Description
There is no O(1) read view for the contracts state, forcing callers to reconstruct it. This issue adds a bounded, read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only entrypoint returning the contracts state without mutating storage.
- Return a sensible default (not a panic) when contracts is unset.
- Reuse stored values rather than recomputing where possible.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-01-view`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: unset state, boundary values.
- Include the full test output in the PR description.

### Example commit message
`feat(contracts): add read view`

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
title: "Add boundary tests for the contracts logic"
labels: type:test, area:contracts, stack:rust, stack:soroban, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover contracts boundaries

### Description
The contracts logic is thinly tested at its boundaries. This issue adds focused boundary and rejection tests.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for the accept/reject boundaries of contracts, asserting exact typed error codes.
- Use the test-utils helpers; assert events where the flow emits them.
- Do not change contract logic unless a defect is found (note it).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-01-boundaries`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: exactly-at boundary, one over, unauthorized caller.
- Include the full test output in the PR description.

### Example commit message
`test(contracts): cover boundaries and rejections`

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
title: "Extract the repeated contracts check into a shared helper"
labels: type:refactor, area:contracts, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate contracts checks

### Description
Multiple entrypoints repeat the same contracts precondition inline. This issue extracts a shared helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract the repeated contracts check into a private helper and route entrypoints through it.
- Behaviour unchanged; same rejections and typed codes.
- No ABI change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-01-helper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: each existing rejection still fires identically.
- Include the full test output in the PR description.

### Example commit message
`refactor(contracts): extract shared check helper`

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
title: "Document the contracts model and its invariants"
labels: type:docs, area:contracts, stack:rust, stack:soroban, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document the contracts model

### Description
The contracts model and its invariants are undocumented, making audits harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/contracts.md` describing the contracts data model, its invariants, and the entrypoints that touch it.
- Cross-reference the code with a worked example; keep it accurate.
- Read the relevant module first.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-01-model`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify invariants against source.
- Include the full test output in the PR description.

### Example commit message
`docs(contracts): document the model and invariants`

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
title: "Emit a dedicated event when contracts state changes"
labels: type:feature, area:contracts, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Emit a contracts event

### Description
State changes to contracts emit no dedicated event, forcing indexers to infer them. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a `symbol_short!` event (<= 9 chars topic) on contracts state change carrying the relevant ids/amounts.
- Do not change fund movement; ensure no topic collision.
- Capture events in tests immediately after the mutating call.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-02-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: no topic collision, event payload correctness.
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
title: "Add a read-only view exposing the current milestones state"
labels: type:feature, area:milestones, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose milestones through a read view

### Description
There is no O(1) read view for the milestones state, forcing callers to reconstruct it. This issue adds a bounded, read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only entrypoint returning the milestones state without mutating storage.
- Return a sensible default (not a panic) when milestones is unset.
- Reuse stored values rather than recomputing where possible.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/milestones-01-view`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: unset state, boundary values.
- Include the full test output in the PR description.

### Example commit message
`feat(milestones): add read view`

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
title: "Add boundary tests for the milestones logic"
labels: type:test, area:milestones, stack:rust, stack:soroban, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover milestones boundaries

### Description
The milestones logic is thinly tested at its boundaries. This issue adds focused boundary and rejection tests.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for the accept/reject boundaries of milestones, asserting exact typed error codes.
- Use the test-utils helpers; assert events where the flow emits them.
- Do not change contract logic unless a defect is found (note it).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/milestones-01-boundaries`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: exactly-at boundary, one over, unauthorized caller.
- Include the full test output in the PR description.

### Example commit message
`test(milestones): cover boundaries and rejections`

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
title: "Extract the repeated milestones check into a shared helper"
labels: type:refactor, area:milestones, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate milestones checks

### Description
Multiple entrypoints repeat the same milestones precondition inline. This issue extracts a shared helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract the repeated milestones check into a private helper and route entrypoints through it.
- Behaviour unchanged; same rejections and typed codes.
- No ABI change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/milestones-01-helper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: each existing rejection still fires identically.
- Include the full test output in the PR description.

### Example commit message
`refactor(milestones): extract shared check helper`

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
title: "Document the milestones model and its invariants"
labels: type:docs, area:milestones, stack:rust, stack:soroban, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document the milestones model

### Description
The milestones model and its invariants are undocumented, making audits harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/milestones.md` describing the milestones data model, its invariants, and the entrypoints that touch it.
- Cross-reference the code with a worked example; keep it accurate.
- Read the relevant module first.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/milestones-01-model`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify invariants against source.
- Include the full test output in the PR description.

### Example commit message
`docs(milestones): document the model and invariants`

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
title: "Emit a dedicated event when milestones state changes"
labels: type:feature, area:milestones, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Emit a milestones event

### Description
State changes to milestones emit no dedicated event, forcing indexers to infer them. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a `symbol_short!` event (<= 9 chars topic) on milestones state change carrying the relevant ids/amounts.
- Do not change fund movement; ensure no topic collision.
- Capture events in tests immediately after the mutating call.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/milestones-02-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: no topic collision, event payload correctness.
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
title: "Add a read-only view exposing the current reputation state"
labels: type:feature, area:reputation, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose reputation through a read view

### Description
There is no O(1) read view for the reputation state, forcing callers to reconstruct it. This issue adds a bounded, read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only entrypoint returning the reputation state without mutating storage.
- Return a sensible default (not a panic) when reputation is unset.
- Reuse stored values rather than recomputing where possible.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/reputation-01-view`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: unset state, boundary values.
- Include the full test output in the PR description.

### Example commit message
`feat(reputation): add read view`

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
title: "Add boundary tests for the reputation logic"
labels: type:test, area:reputation, stack:rust, stack:soroban, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover reputation boundaries

### Description
The reputation logic is thinly tested at its boundaries. This issue adds focused boundary and rejection tests.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for the accept/reject boundaries of reputation, asserting exact typed error codes.
- Use the test-utils helpers; assert events where the flow emits them.
- Do not change contract logic unless a defect is found (note it).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/reputation-01-boundaries`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: exactly-at boundary, one over, unauthorized caller.
- Include the full test output in the PR description.

### Example commit message
`test(reputation): cover boundaries and rejections`

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
title: "Extract the repeated reputation check into a shared helper"
labels: type:refactor, area:reputation, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate reputation checks

### Description
Multiple entrypoints repeat the same reputation precondition inline. This issue extracts a shared helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract the repeated reputation check into a private helper and route entrypoints through it.
- Behaviour unchanged; same rejections and typed codes.
- No ABI change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/reputation-01-helper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: each existing rejection still fires identically.
- Include the full test output in the PR description.

### Example commit message
`refactor(reputation): extract shared check helper`

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
title: "Document the reputation model and its invariants"
labels: type:docs, area:reputation, stack:rust, stack:soroban, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document the reputation model

### Description
The reputation model and its invariants are undocumented, making audits harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/reputation.md` describing the reputation data model, its invariants, and the entrypoints that touch it.
- Cross-reference the code with a worked example; keep it accurate.
- Read the relevant module first.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/reputation-01-model`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify invariants against source.
- Include the full test output in the PR description.

### Example commit message
`docs(reputation): document the model and invariants`

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
title: "Emit a dedicated event when reputation state changes"
labels: type:feature, area:reputation, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Emit a reputation event

### Description
State changes to reputation emit no dedicated event, forcing indexers to infer them. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a `symbol_short!` event (<= 9 chars topic) on reputation state change carrying the relevant ids/amounts.
- Do not change fund movement; ensure no topic collision.
- Capture events in tests immediately after the mutating call.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/reputation-02-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: no topic collision, event payload correctness.
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
title: "Add a read-only view exposing the current disputes state"
labels: type:feature, area:disputes, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose disputes through a read view

### Description
There is no O(1) read view for the disputes state, forcing callers to reconstruct it. This issue adds a bounded, read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only entrypoint returning the disputes state without mutating storage.
- Return a sensible default (not a panic) when disputes is unset.
- Reuse stored values rather than recomputing where possible.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/disputes-01-view`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: unset state, boundary values.
- Include the full test output in the PR description.

### Example commit message
`feat(disputes): add read view`

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
title: "Add boundary tests for the disputes logic"
labels: type:test, area:disputes, stack:rust, stack:soroban, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover disputes boundaries

### Description
The disputes logic is thinly tested at its boundaries. This issue adds focused boundary and rejection tests.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for the accept/reject boundaries of disputes, asserting exact typed error codes.
- Use the test-utils helpers; assert events where the flow emits them.
- Do not change contract logic unless a defect is found (note it).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/disputes-01-boundaries`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: exactly-at boundary, one over, unauthorized caller.
- Include the full test output in the PR description.

### Example commit message
`test(disputes): cover boundaries and rejections`

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
title: "Extract the repeated disputes check into a shared helper"
labels: type:refactor, area:disputes, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate disputes checks

### Description
Multiple entrypoints repeat the same disputes precondition inline. This issue extracts a shared helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract the repeated disputes check into a private helper and route entrypoints through it.
- Behaviour unchanged; same rejections and typed codes.
- No ABI change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/disputes-01-helper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: each existing rejection still fires identically.
- Include the full test output in the PR description.

### Example commit message
`refactor(disputes): extract shared check helper`

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
title: "Document the disputes model and its invariants"
labels: type:docs, area:disputes, stack:rust, stack:soroban, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document the disputes model

### Description
The disputes model and its invariants are undocumented, making audits harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/disputes.md` describing the disputes data model, its invariants, and the entrypoints that touch it.
- Cross-reference the code with a worked example; keep it accurate.
- Read the relevant module first.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/disputes-01-model`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify invariants against source.
- Include the full test output in the PR description.

### Example commit message
`docs(disputes): document the model and invariants`

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
title: "Emit a dedicated event when disputes state changes"
labels: type:feature, area:disputes, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Emit a disputes event

### Description
State changes to disputes emit no dedicated event, forcing indexers to infer them. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a `symbol_short!` event (<= 9 chars topic) on disputes state change carrying the relevant ids/amounts.
- Do not change fund movement; ensure no topic collision.
- Capture events in tests immediately after the mutating call.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/disputes-02-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: no topic collision, event payload correctness.
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
title: "Add a read-only view exposing the current arbiter state"
labels: type:feature, area:arbiter, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose arbiter through a read view

### Description
There is no O(1) read view for the arbiter state, forcing callers to reconstruct it. This issue adds a bounded, read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only entrypoint returning the arbiter state without mutating storage.
- Return a sensible default (not a panic) when arbiter is unset.
- Reuse stored values rather than recomputing where possible.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/arbiter-01-view`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: unset state, boundary values.
- Include the full test output in the PR description.

### Example commit message
`feat(arbiter): add read view`

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
title: "Add boundary tests for the arbiter logic"
labels: type:test, area:arbiter, stack:rust, stack:soroban, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover arbiter boundaries

### Description
The arbiter logic is thinly tested at its boundaries. This issue adds focused boundary and rejection tests.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for the accept/reject boundaries of arbiter, asserting exact typed error codes.
- Use the test-utils helpers; assert events where the flow emits them.
- Do not change contract logic unless a defect is found (note it).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/arbiter-01-boundaries`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: exactly-at boundary, one over, unauthorized caller.
- Include the full test output in the PR description.

### Example commit message
`test(arbiter): cover boundaries and rejections`

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
title: "Extract the repeated arbiter check into a shared helper"
labels: type:refactor, area:arbiter, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate arbiter checks

### Description
Multiple entrypoints repeat the same arbiter precondition inline. This issue extracts a shared helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract the repeated arbiter check into a private helper and route entrypoints through it.
- Behaviour unchanged; same rejections and typed codes.
- No ABI change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/arbiter-01-helper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: each existing rejection still fires identically.
- Include the full test output in the PR description.

### Example commit message
`refactor(arbiter): extract shared check helper`

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
title: "Document the arbiter model and its invariants"
labels: type:docs, area:arbiter, stack:rust, stack:soroban, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document the arbiter model

### Description
The arbiter model and its invariants are undocumented, making audits harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/arbiter.md` describing the arbiter data model, its invariants, and the entrypoints that touch it.
- Cross-reference the code with a worked example; keep it accurate.
- Read the relevant module first.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/arbiter-01-model`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify invariants against source.
- Include the full test output in the PR description.

### Example commit message
`docs(arbiter): document the model and invariants`

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
title: "Emit a dedicated event when arbiter state changes"
labels: type:feature, area:arbiter, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Emit a arbiter event

### Description
State changes to arbiter emit no dedicated event, forcing indexers to infer them. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a `symbol_short!` event (<= 9 chars topic) on arbiter state change carrying the relevant ids/amounts.
- Do not change fund movement; ensure no topic collision.
- Capture events in tests immediately after the mutating call.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/arbiter-02-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: no topic collision, event payload correctness.
- Include the full test output in the PR description.

### Example commit message
`feat(arbiter): emit state-change event`

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
title: "Add a read-only view exposing the current settlement state"
labels: type:feature, area:settlement, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose settlement through a read view

### Description
There is no O(1) read view for the settlement state, forcing callers to reconstruct it. This issue adds a bounded, read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only entrypoint returning the settlement state without mutating storage.
- Return a sensible default (not a panic) when settlement is unset.
- Reuse stored values rather than recomputing where possible.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/settlement-01-view`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: unset state, boundary values.
- Include the full test output in the PR description.

### Example commit message
`feat(settlement): add read view`

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
title: "Add boundary tests for the settlement logic"
labels: type:test, area:settlement, stack:rust, stack:soroban, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover settlement boundaries

### Description
The settlement logic is thinly tested at its boundaries. This issue adds focused boundary and rejection tests.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for the accept/reject boundaries of settlement, asserting exact typed error codes.
- Use the test-utils helpers; assert events where the flow emits them.
- Do not change contract logic unless a defect is found (note it).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/settlement-01-boundaries`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: exactly-at boundary, one over, unauthorized caller.
- Include the full test output in the PR description.

### Example commit message
`test(settlement): cover boundaries and rejections`

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
title: "Extract the repeated settlement check into a shared helper"
labels: type:refactor, area:settlement, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate settlement checks

### Description
Multiple entrypoints repeat the same settlement precondition inline. This issue extracts a shared helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract the repeated settlement check into a private helper and route entrypoints through it.
- Behaviour unchanged; same rejections and typed codes.
- No ABI change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/settlement-01-helper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: each existing rejection still fires identically.
- Include the full test output in the PR description.

### Example commit message
`refactor(settlement): extract shared check helper`

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
title: "Document the settlement model and its invariants"
labels: type:docs, area:settlement, stack:rust, stack:soroban, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document the settlement model

### Description
The settlement model and its invariants are undocumented, making audits harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/settlement.md` describing the settlement data model, its invariants, and the entrypoints that touch it.
- Cross-reference the code with a worked example; keep it accurate.
- Read the relevant module first.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/settlement-01-model`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify invariants against source.
- Include the full test output in the PR description.

### Example commit message
`docs(settlement): document the model and invariants`

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
title: "Emit a dedicated event when settlement state changes"
labels: type:feature, area:settlement, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Emit a settlement event

### Description
State changes to settlement emit no dedicated event, forcing indexers to infer them. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a `symbol_short!` event (<= 9 chars topic) on settlement state change carrying the relevant ids/amounts.
- Do not change fund movement; ensure no topic collision.
- Capture events in tests immediately after the mutating call.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/settlement-02-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: no topic collision, event payload correctness.
- Include the full test output in the PR description.

### Example commit message
`feat(settlement): emit state-change event`

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
title: "Add a read-only view exposing the current storage state"
labels: type:feature, area:storage, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose storage through a read view

### Description
There is no O(1) read view for the storage state, forcing callers to reconstruct it. This issue adds a bounded, read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only entrypoint returning the storage state without mutating storage.
- Return a sensible default (not a panic) when storage is unset.
- Reuse stored values rather than recomputing where possible.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/storage-01-view`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: unset state, boundary values.
- Include the full test output in the PR description.

### Example commit message
`feat(storage): add read view`

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
title: "Add boundary tests for the storage logic"
labels: type:test, area:storage, stack:rust, stack:soroban, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover storage boundaries

### Description
The storage logic is thinly tested at its boundaries. This issue adds focused boundary and rejection tests.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for the accept/reject boundaries of storage, asserting exact typed error codes.
- Use the test-utils helpers; assert events where the flow emits them.
- Do not change contract logic unless a defect is found (note it).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/storage-01-boundaries`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: exactly-at boundary, one over, unauthorized caller.
- Include the full test output in the PR description.

### Example commit message
`test(storage): cover boundaries and rejections`

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
title: "Extract the repeated storage check into a shared helper"
labels: type:refactor, area:storage, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate storage checks

### Description
Multiple entrypoints repeat the same storage precondition inline. This issue extracts a shared helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract the repeated storage check into a private helper and route entrypoints through it.
- Behaviour unchanged; same rejections and typed codes.
- No ABI change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/storage-01-helper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: each existing rejection still fires identically.
- Include the full test output in the PR description.

### Example commit message
`refactor(storage): extract shared check helper`

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
title: "Document the storage model and its invariants"
labels: type:docs, area:storage, stack:rust, stack:soroban, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document the storage model

### Description
The storage model and its invariants are undocumented, making audits harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/storage.md` describing the storage data model, its invariants, and the entrypoints that touch it.
- Cross-reference the code with a worked example; keep it accurate.
- Read the relevant module first.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/storage-01-model`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify invariants against source.
- Include the full test output in the PR description.

### Example commit message
`docs(storage): document the model and invariants`

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
title: "Emit a dedicated event when storage state changes"
labels: type:feature, area:storage, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Emit a storage event

### Description
State changes to storage emit no dedicated event, forcing indexers to infer them. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a `symbol_short!` event (<= 9 chars topic) on storage state change carrying the relevant ids/amounts.
- Do not change fund movement; ensure no topic collision.
- Capture events in tests immediately after the mutating call.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/storage-02-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: no topic collision, event payload correctness.
- Include the full test output in the PR description.

### Example commit message
`feat(storage): emit state-change event`

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
title: "Add a read-only view exposing the current events state"
labels: type:feature, area:events, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose events through a read view

### Description
There is no O(1) read view for the events state, forcing callers to reconstruct it. This issue adds a bounded, read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only entrypoint returning the events state without mutating storage.
- Return a sensible default (not a panic) when events is unset.
- Reuse stored values rather than recomputing where possible.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/events-01-view`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: unset state, boundary values.
- Include the full test output in the PR description.

### Example commit message
`feat(events): add read view`

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
title: "Add boundary tests for the events logic"
labels: type:test, area:events, stack:rust, stack:soroban, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover events boundaries

### Description
The events logic is thinly tested at its boundaries. This issue adds focused boundary and rejection tests.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for the accept/reject boundaries of events, asserting exact typed error codes.
- Use the test-utils helpers; assert events where the flow emits them.
- Do not change contract logic unless a defect is found (note it).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/events-01-boundaries`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: exactly-at boundary, one over, unauthorized caller.
- Include the full test output in the PR description.

### Example commit message
`test(events): cover boundaries and rejections`

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
title: "Extract the repeated events check into a shared helper"
labels: type:refactor, area:events, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate events checks

### Description
Multiple entrypoints repeat the same events precondition inline. This issue extracts a shared helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract the repeated events check into a private helper and route entrypoints through it.
- Behaviour unchanged; same rejections and typed codes.
- No ABI change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/events-01-helper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: each existing rejection still fires identically.
- Include the full test output in the PR description.

### Example commit message
`refactor(events): extract shared check helper`

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
title: "Document the events model and its invariants"
labels: type:docs, area:events, stack:rust, stack:soroban, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document the events model

### Description
The events model and its invariants are undocumented, making audits harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/events.md` describing the events data model, its invariants, and the entrypoints that touch it.
- Cross-reference the code with a worked example; keep it accurate.
- Read the relevant module first.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/events-01-model`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify invariants against source.
- Include the full test output in the PR description.

### Example commit message
`docs(events): document the model and invariants`

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
title: "Emit a dedicated event when events state changes"
labels: type:feature, area:events, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Emit a events event

### Description
State changes to events emit no dedicated event, forcing indexers to infer them. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a `symbol_short!` event (<= 9 chars topic) on events state change carrying the relevant ids/amounts.
- Do not change fund movement; ensure no topic collision.
- Capture events in tests immediately after the mutating call.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/events-02-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: no topic collision, event payload correctness.
- Include the full test output in the PR description.

### Example commit message
`feat(events): emit state-change event`

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
title: "Add a read-only view exposing the current authorization state"
labels: type:feature, area:authorization, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose authorization through a read view

### Description
There is no O(1) read view for the authorization state, forcing callers to reconstruct it. This issue adds a bounded, read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only entrypoint returning the authorization state without mutating storage.
- Return a sensible default (not a panic) when authorization is unset.
- Reuse stored values rather than recomputing where possible.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/authorization-01-view`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: unset state, boundary values.
- Include the full test output in the PR description.

### Example commit message
`feat(authorization): add read view`

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
title: "Add boundary tests for the authorization logic"
labels: type:test, area:authorization, stack:rust, stack:soroban, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover authorization boundaries

### Description
The authorization logic is thinly tested at its boundaries. This issue adds focused boundary and rejection tests.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for the accept/reject boundaries of authorization, asserting exact typed error codes.
- Use the test-utils helpers; assert events where the flow emits them.
- Do not change contract logic unless a defect is found (note it).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/authorization-01-boundaries`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: exactly-at boundary, one over, unauthorized caller.
- Include the full test output in the PR description.

### Example commit message
`test(authorization): cover boundaries and rejections`

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
title: "Extract the repeated authorization check into a shared helper"
labels: type:refactor, area:authorization, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate authorization checks

### Description
Multiple entrypoints repeat the same authorization precondition inline. This issue extracts a shared helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract the repeated authorization check into a private helper and route entrypoints through it.
- Behaviour unchanged; same rejections and typed codes.
- No ABI change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/authorization-01-helper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: each existing rejection still fires identically.
- Include the full test output in the PR description.

### Example commit message
`refactor(authorization): extract shared check helper`

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
title: "Document the authorization model and its invariants"
labels: type:docs, area:authorization, stack:rust, stack:soroban, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document the authorization model

### Description
The authorization model and its invariants are undocumented, making audits harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/authorization.md` describing the authorization data model, its invariants, and the entrypoints that touch it.
- Cross-reference the code with a worked example; keep it accurate.
- Read the relevant module first.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/authorization-01-model`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify invariants against source.
- Include the full test output in the PR description.

### Example commit message
`docs(authorization): document the model and invariants`

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
title: "Emit a dedicated event when authorization state changes"
labels: type:feature, area:authorization, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Emit a authorization event

### Description
State changes to authorization emit no dedicated event, forcing indexers to infer them. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a `symbol_short!` event (<= 9 chars topic) on authorization state change carrying the relevant ids/amounts.
- Do not change fund movement; ensure no topic collision.
- Capture events in tests immediately after the mutating call.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/authorization-02-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: no topic collision, event payload correctness.
- Include the full test output in the PR description.

### Example commit message
`feat(authorization): emit state-change event`

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
title: "Add a read-only view exposing the current escrow state"
labels: type:feature, area:escrow, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose escrow through a read view

### Description
There is no O(1) read view for the escrow state, forcing callers to reconstruct it. This issue adds a bounded, read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only entrypoint returning the escrow state without mutating storage.
- Return a sensible default (not a panic) when escrow is unset.
- Reuse stored values rather than recomputing where possible.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/escrow-01-view`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: unset state, boundary values.
- Include the full test output in the PR description.

### Example commit message
`feat(escrow): add read view`

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
title: "Add boundary tests for the escrow logic"
labels: type:test, area:escrow, stack:rust, stack:soroban, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Cover escrow boundaries

### Description
The escrow logic is thinly tested at its boundaries. This issue adds focused boundary and rejection tests.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for the accept/reject boundaries of escrow, asserting exact typed error codes.
- Use the test-utils helpers; assert events where the flow emits them.
- Do not change contract logic unless a defect is found (note it).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/escrow-01-boundaries`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: exactly-at boundary, one over, unauthorized caller.
- Include the full test output in the PR description.

### Example commit message
`test(escrow): cover boundaries and rejections`

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
title: "Extract the repeated escrow check into a shared helper"
labels: type:refactor, area:escrow, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Deduplicate escrow checks

### Description
Multiple entrypoints repeat the same escrow precondition inline. This issue extracts a shared helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract the repeated escrow check into a private helper and route entrypoints through it.
- Behaviour unchanged; same rejections and typed codes.
- No ABI change.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/escrow-01-helper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: each existing rejection still fires identically.
- Include the full test output in the PR description.

### Example commit message
`refactor(escrow): extract shared check helper`

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
title: "Document the escrow model and its invariants"
labels: type:docs, area:escrow, stack:rust, stack:soroban, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document the escrow model

### Description
The escrow model and its invariants are undocumented, making audits harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/escrow.md` describing the escrow data model, its invariants, and the entrypoints that touch it.
- Cross-reference the code with a worked example; keep it accurate.
- Read the relevant module first.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/escrow-01-model`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify invariants against source.
- Include the full test output in the PR description.

### Example commit message
`docs(escrow): document the model and invariants`

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
title: "Emit a dedicated event when escrow state changes"
labels: type:feature, area:escrow, stack:rust, stack:soroban, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Emit a escrow event

### Description
State changes to escrow emit no dedicated event, forcing indexers to infer them. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Emit a `symbol_short!` event (<= 9 chars topic) on escrow state change carrying the relevant ids/amounts.
- Do not change fund movement; ensure no topic collision.
- Capture events in tests immediately after the mutating call.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/escrow-02-event`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: no topic collision, event payload correctness.
- Include the full test output in the PR description.

### Example commit message
`feat(escrow): emit state-change event`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
