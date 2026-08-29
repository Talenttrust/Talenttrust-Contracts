---
type: Feature
title: "Add a read view exposing contracts configuration"
labels: type:feature, area:contracts, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose contracts config

### Description
Callers can't read the current contracts configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only view returning the contracts configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(contracts): add config read view`

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
title: "Add tests for contracts event topics and payloads"
labels: type:test, area:contracts, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test contracts events

### Description
contracts's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests capturing contracts's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(contracts): cover event topics/payloads`

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
title: "Return a typed struct from contracts instead of a tuple"
labels: type:refactor, area:contracts, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type contracts return

### Description
contracts returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Replace contracts's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(contracts): return a typed struct`

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
title: "Add an admin setter to update contracts parameters within bounds"
labels: type:feature, area:contracts, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure contracts

### Description
contracts parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add an admin-guarded setter for the contracts parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(contracts): add admin parameter setter`

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
title: "Document contracts error codes and their meanings"
labels: type:docs, area:contracts, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document contracts errors

### Description
contracts's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/contracts-errors.md` listing each contracts EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(contracts): document error codes`

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
title: "Add a read view exposing milestones configuration"
labels: type:feature, area:milestones, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose milestones config

### Description
Callers can't read the current milestones configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only view returning the milestones configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/milestones-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(milestones): add config read view`

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
title: "Add tests for milestones event topics and payloads"
labels: type:test, area:milestones, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test milestones events

### Description
milestones's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests capturing milestones's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/milestones-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(milestones): cover event topics/payloads`

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
title: "Return a typed struct from milestones instead of a tuple"
labels: type:refactor, area:milestones, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type milestones return

### Description
milestones returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Replace milestones's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/milestones-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(milestones): return a typed struct`

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
title: "Add an admin setter to update milestones parameters within bounds"
labels: type:feature, area:milestones, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure milestones

### Description
milestones parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add an admin-guarded setter for the milestones parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/milestones-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(milestones): add admin parameter setter`

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
title: "Document milestones error codes and their meanings"
labels: type:docs, area:milestones, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document milestones errors

### Description
milestones's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/milestones-errors.md` listing each milestones EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/milestones-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(milestones): document error codes`

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
title: "Add a read view exposing reputation configuration"
labels: type:feature, area:reputation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose reputation config

### Description
Callers can't read the current reputation configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only view returning the reputation configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/reputation-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(reputation): add config read view`

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
title: "Add tests for reputation event topics and payloads"
labels: type:test, area:reputation, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test reputation events

### Description
reputation's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests capturing reputation's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/reputation-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(reputation): cover event topics/payloads`

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
title: "Return a typed struct from reputation instead of a tuple"
labels: type:refactor, area:reputation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type reputation return

### Description
reputation returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Replace reputation's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/reputation-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(reputation): return a typed struct`

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
title: "Add an admin setter to update reputation parameters within bounds"
labels: type:feature, area:reputation, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure reputation

### Description
reputation parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add an admin-guarded setter for the reputation parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/reputation-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(reputation): add admin parameter setter`

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
title: "Document reputation error codes and their meanings"
labels: type:docs, area:reputation, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document reputation errors

### Description
reputation's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/reputation-errors.md` listing each reputation EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/reputation-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(reputation): document error codes`

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
title: "Add a read view exposing disputes configuration"
labels: type:feature, area:disputes, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose disputes config

### Description
Callers can't read the current disputes configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only view returning the disputes configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/disputes-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(disputes): add config read view`

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
title: "Add tests for disputes event topics and payloads"
labels: type:test, area:disputes, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test disputes events

### Description
disputes's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests capturing disputes's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/disputes-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(disputes): cover event topics/payloads`

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
title: "Return a typed struct from disputes instead of a tuple"
labels: type:refactor, area:disputes, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type disputes return

### Description
disputes returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Replace disputes's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/disputes-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(disputes): return a typed struct`

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
title: "Add an admin setter to update disputes parameters within bounds"
labels: type:feature, area:disputes, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure disputes

### Description
disputes parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add an admin-guarded setter for the disputes parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/disputes-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(disputes): add admin parameter setter`

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
title: "Document disputes error codes and their meanings"
labels: type:docs, area:disputes, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document disputes errors

### Description
disputes's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/disputes-errors.md` listing each disputes EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/disputes-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(disputes): document error codes`

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
title: "Add a read view exposing escrow configuration"
labels: type:feature, area:escrow, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose escrow config

### Description
Callers can't read the current escrow configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only view returning the escrow configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/escrow-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(escrow): add config read view`

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
title: "Add tests for escrow event topics and payloads"
labels: type:test, area:escrow, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test escrow events

### Description
escrow's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests capturing escrow's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/escrow-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(escrow): cover event topics/payloads`

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
title: "Return a typed struct from escrow instead of a tuple"
labels: type:refactor, area:escrow, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type escrow return

### Description
escrow returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Replace escrow's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/escrow-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(escrow): return a typed struct`

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
title: "Add an admin setter to update escrow parameters within bounds"
labels: type:feature, area:escrow, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure escrow

### Description
escrow parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add an admin-guarded setter for the escrow parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/escrow-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(escrow): add admin parameter setter`

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
title: "Document escrow error codes and their meanings"
labels: type:docs, area:escrow, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document escrow errors

### Description
escrow's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/escrow-errors.md` listing each escrow EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/escrow-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(escrow): document error codes`

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
title: "Add a read view exposing settlement configuration"
labels: type:feature, area:settlement, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose settlement config

### Description
Callers can't read the current settlement configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only view returning the settlement configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/settlement-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(settlement): add config read view`

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
title: "Add tests for settlement event topics and payloads"
labels: type:test, area:settlement, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test settlement events

### Description
settlement's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests capturing settlement's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/settlement-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(settlement): cover event topics/payloads`

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
title: "Return a typed struct from settlement instead of a tuple"
labels: type:refactor, area:settlement, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type settlement return

### Description
settlement returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Replace settlement's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/settlement-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(settlement): return a typed struct`

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
title: "Add an admin setter to update settlement parameters within bounds"
labels: type:feature, area:settlement, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure settlement

### Description
settlement parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add an admin-guarded setter for the settlement parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/settlement-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(settlement): add admin parameter setter`

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
title: "Document settlement error codes and their meanings"
labels: type:docs, area:settlement, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document settlement errors

### Description
settlement's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/settlement-errors.md` listing each settlement EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/settlement-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(settlement): document error codes`

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
title: "Add a read view exposing arbiter configuration"
labels: type:feature, area:arbiter, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Expose arbiter config

### Description
Callers can't read the current arbiter configuration. This issue adds a read-only config view.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a read-only view returning the arbiter configuration values without mutating storage.
- Return sensible defaults before init.
- Cover the values and pre-init default in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/arbiter-51-configview`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: values after set, default before init.
- Include the full test output in the PR description.

### Example commit message
`feat(arbiter): add config read view`

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
title: "Add tests for arbiter event topics and payloads"
labels: type:test, area:arbiter, stack:rust, stack:soroban, priority:high, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Test arbiter events

### Description
arbiter's emitted events aren't asserted, so topic/payload drift slips through. This issue adds coverage.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add tests capturing arbiter's events and asserting the topic symbols and payload fields.
- Capture events immediately after the emitting call (buffer holds latest invocation).
- Assert no topic collision.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/arbiter-51-events`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: topic correctness, payload fields, no collision.
- Include the full test output in the PR description.

### Example commit message
`test(arbiter): cover event topics/payloads`

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
title: "Return a typed struct from arbiter instead of a tuple"
labels: type:refactor, area:arbiter, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Type arbiter return

### Description
arbiter returns an opaque tuple, hurting readability. This issue returns a named struct.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Replace arbiter's tuple return with a documented struct; update call sites and tests.
- Behaviour unchanged; ABI adjusted intentionally.
- Tests pass.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/arbiter-51-structret`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: fields match tuple, call sites updated.
- Include the full test output in the PR description.

### Example commit message
`refactor(arbiter): return a typed struct`

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
title: "Add an admin setter to update arbiter parameters within bounds"
labels: type:feature, area:arbiter, stack:rust, stack:soroban, priority:medium, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Configure arbiter

### Description
arbiter parameters are fixed at init. This issue adds an admin setter with bounds validation.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add an admin-guarded setter for the arbiter parameters, validating bounds and rejecting out-of-range with a typed error.
- Emit an event on change.
- Cover set, bounds, and non-admin in tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/arbiter-52-setter`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: in-bounds set, over-bounds rejected, non-admin rejected.
- Include the full test output in the PR description.

### Example commit message
`feat(arbiter): add admin parameter setter`

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
title: "Document arbiter error codes and their meanings"
labels: type:docs, area:arbiter, stack:rust, stack:soroban, priority:low, Stellar Wave, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---

## Document arbiter errors

### Description
arbiter's typed error codes aren't documented, making integration harder. This issue documents them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `docs/arbiter-errors.md` listing each arbiter EscrowError code, when it fires, and how to avoid it.
- Cross-reference the entrypoints.
- Keep accurate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/arbiter-51-errdocs`
- Implement changes
  - **Write code in:** the relevant module.
  - **Write comprehensive tests in:** cover the new behaviour and edge cases.
- Test and commit

### Test and commit
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`.
- Cover edge cases: n/a — verify codes against source.
- Include the full test output in the PR description.

### Example commit message
`docs(arbiter): document error codes`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
