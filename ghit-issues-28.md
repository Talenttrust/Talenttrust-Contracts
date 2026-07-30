---
type: Feature
title: "Add a version/metadata view to contracts"
labels: type:feature, area:contracts, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Version contracts

### Description
Callers can't query contracts's deployed version/metadata. This issue adds a read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only view returning contracts's version/metadata (e.g. schema version) without mutating storage.
- Return a sane default before init.
- Cover the value in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-71-version`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: value after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(contracts): add version view`

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
title: "Add boundary/fuzz-style tests for contracts"
labels: type:test, area:contracts, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Boundary-test contracts

### Description
contracts's numeric/length boundaries aren't exhaustively tested. This issue adds boundary cases.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for contracts at min, max, zero, and over-limit inputs asserting typed errors where expected.
- Keep runs bounded.
- Note any unguarded boundary found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-71-boundary`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: min, max, zero, over-limit.
- Include the full test output in the PR description.

### Example commit message
`test(contracts): add boundary tests`

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
title: "Extract contracts validation into a helper"
labels: type:refactor, area:contracts, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Helper for contracts

### Description
contracts repeats inline validation. This issue extracts a shared validation helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract contracts's repeated validation into a helper returning a typed error; reuse it at each call site.
- Behaviour identical.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-71-valhelper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same rejections, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(contracts): extract validation helper`

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
title: "Add an upgrade-authorization check to contracts"
labels: type:feature, area:contracts, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Guard contracts upgrade

### Description
contracts's upgrade path lacks an explicit admin authorization check. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Require admin authorization for contracts's upgrade/migration entrypoint, rejecting others with the typed error.
- Emit an event on upgrade.
- Cover admin-allowed and non-admin-rejected in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-72-upgradeauth`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: admin allowed, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(contracts): guard upgrade authorization`

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
title: "Add a state-diagram note for contracts"
labels: type:docs, area:contracts, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Diagram contracts states

### Description
contracts's state machine isn't documented. This issue adds a state-diagram note.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/contracts-states.md` with a diagram of contracts's states and allowed transitions.
- Cross-reference the entrypoints enforcing them.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-71-states`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(contracts): add state diagram`

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
title: "Add a version/metadata view to milestones"
labels: type:feature, area:milestones, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Version milestones

### Description
Callers can't query milestones's deployed version/metadata. This issue adds a read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only view returning milestones's version/metadata (e.g. schema version) without mutating storage.
- Return a sane default before init.
- Cover the value in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/milestones-71-version`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: value after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(milestones): add version view`

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
title: "Add boundary/fuzz-style tests for milestones"
labels: type:test, area:milestones, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Boundary-test milestones

### Description
milestones's numeric/length boundaries aren't exhaustively tested. This issue adds boundary cases.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for milestones at min, max, zero, and over-limit inputs asserting typed errors where expected.
- Keep runs bounded.
- Note any unguarded boundary found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/milestones-71-boundary`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: min, max, zero, over-limit.
- Include the full test output in the PR description.

### Example commit message
`test(milestones): add boundary tests`

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
title: "Extract milestones validation into a helper"
labels: type:refactor, area:milestones, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Helper for milestones

### Description
milestones repeats inline validation. This issue extracts a shared validation helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract milestones's repeated validation into a helper returning a typed error; reuse it at each call site.
- Behaviour identical.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/milestones-71-valhelper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same rejections, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(milestones): extract validation helper`

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
title: "Add an upgrade-authorization check to milestones"
labels: type:feature, area:milestones, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Guard milestones upgrade

### Description
milestones's upgrade path lacks an explicit admin authorization check. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Require admin authorization for milestones's upgrade/migration entrypoint, rejecting others with the typed error.
- Emit an event on upgrade.
- Cover admin-allowed and non-admin-rejected in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/milestones-72-upgradeauth`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: admin allowed, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(milestones): guard upgrade authorization`

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
title: "Add a state-diagram note for milestones"
labels: type:docs, area:milestones, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Diagram milestones states

### Description
milestones's state machine isn't documented. This issue adds a state-diagram note.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/milestones-states.md` with a diagram of milestones's states and allowed transitions.
- Cross-reference the entrypoints enforcing them.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/milestones-71-states`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(milestones): add state diagram`

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
title: "Add a version/metadata view to reputation"
labels: type:feature, area:reputation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Version reputation

### Description
Callers can't query reputation's deployed version/metadata. This issue adds a read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only view returning reputation's version/metadata (e.g. schema version) without mutating storage.
- Return a sane default before init.
- Cover the value in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/reputation-71-version`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: value after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(reputation): add version view`

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
title: "Add boundary/fuzz-style tests for reputation"
labels: type:test, area:reputation, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Boundary-test reputation

### Description
reputation's numeric/length boundaries aren't exhaustively tested. This issue adds boundary cases.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for reputation at min, max, zero, and over-limit inputs asserting typed errors where expected.
- Keep runs bounded.
- Note any unguarded boundary found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/reputation-71-boundary`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: min, max, zero, over-limit.
- Include the full test output in the PR description.

### Example commit message
`test(reputation): add boundary tests`

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
title: "Extract reputation validation into a helper"
labels: type:refactor, area:reputation, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Helper for reputation

### Description
reputation repeats inline validation. This issue extracts a shared validation helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract reputation's repeated validation into a helper returning a typed error; reuse it at each call site.
- Behaviour identical.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/reputation-71-valhelper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same rejections, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(reputation): extract validation helper`

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
title: "Add an upgrade-authorization check to reputation"
labels: type:feature, area:reputation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Guard reputation upgrade

### Description
reputation's upgrade path lacks an explicit admin authorization check. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Require admin authorization for reputation's upgrade/migration entrypoint, rejecting others with the typed error.
- Emit an event on upgrade.
- Cover admin-allowed and non-admin-rejected in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/reputation-72-upgradeauth`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: admin allowed, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(reputation): guard upgrade authorization`

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
title: "Add a state-diagram note for reputation"
labels: type:docs, area:reputation, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Diagram reputation states

### Description
reputation's state machine isn't documented. This issue adds a state-diagram note.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/reputation-states.md` with a diagram of reputation's states and allowed transitions.
- Cross-reference the entrypoints enforcing them.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/reputation-71-states`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(reputation): add state diagram`

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
title: "Add a version/metadata view to disputes"
labels: type:feature, area:disputes, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Version disputes

### Description
Callers can't query disputes's deployed version/metadata. This issue adds a read-only view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only view returning disputes's version/metadata (e.g. schema version) without mutating storage.
- Return a sane default before init.
- Cover the value in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/disputes-71-version`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: value after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(disputes): add version view`

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
title: "Add boundary/fuzz-style tests for disputes"
labels: type:test, area:disputes, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Boundary-test disputes

### Description
disputes's numeric/length boundaries aren't exhaustively tested. This issue adds boundary cases.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests for disputes at min, max, zero, and over-limit inputs asserting typed errors where expected.
- Keep runs bounded.
- Note any unguarded boundary found.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/disputes-71-boundary`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: min, max, zero, over-limit.
- Include the full test output in the PR description.

### Example commit message
`test(disputes): add boundary tests`

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
title: "Extract disputes validation into a helper"
labels: type:refactor, area:disputes, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Helper for disputes

### Description
disputes repeats inline validation. This issue extracts a shared validation helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extract disputes's repeated validation into a helper returning a typed error; reuse it at each call site.
- Behaviour identical.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/disputes-71-valhelper`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: same rejections, tests pass.
- Include the full test output in the PR description.

### Example commit message
`refactor(disputes): extract validation helper`

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
title: "Add an upgrade-authorization check to disputes"
labels: type:feature, area:disputes, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Guard disputes upgrade

### Description
disputes's upgrade path lacks an explicit admin authorization check. This issue adds one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Require admin authorization for disputes's upgrade/migration entrypoint, rejecting others with the typed error.
- Emit an event on upgrade.
- Cover admin-allowed and non-admin-rejected in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/disputes-72-upgradeauth`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: admin allowed, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(disputes): guard upgrade authorization`

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
title: "Add a state-diagram note for disputes"
labels: type:docs, area:disputes, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Diagram disputes states

### Description
disputes's state machine isn't documented. This issue adds a state-diagram note.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/disputes-states.md` with a diagram of disputes's states and allowed transitions.
- Cross-reference the entrypoints enforcing them.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/disputes-71-states`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify against source.
- Include the full test output in the PR description.

### Example commit message
`docs(disputes): add state diagram`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
