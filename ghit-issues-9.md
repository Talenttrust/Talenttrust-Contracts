---
type: Feature
title: "Add a paginated list_contracts_by_status reader for operations dashboards"
labels: type:feature, area:indexer-views, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add a paginated list_contracts_by_status reader for operations dashboards

### Description
The escrow contract exposes `get_contract`, `contract_exists`, and `get_contract_summary`, but there is no way to enumerate contracts by lifecycle state. Operators cannot cheaply answer "which escrows are currently `Disputed` or `Funded`" without probing every id up to `get_next_contract_id`. Add a status-filtered, paginated reader backed by a maintained status index.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `list_contracts_by_status(env, status: ContractStatus, start: u32, limit: u32) -> Vec<u32>` returning contract ids.
- Maintain a `DataKey` status index updated on every status transition in `create_contract`, `deposit_funds`, `release_milestone`, `cancel_contract`, `raise_dispute`, and `finalize_contract`.
- Bound `limit` to a small constant and extend index TTL via `ttl::store_with_ttl` on write.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-list-contracts-by-status`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/participant_index_pagination.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(escrow): add paginated list_contracts_by_status reader`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add an extend_contract_lifetime entrypoint for participant-funded storage TTL top-ups"
labels: type:feature, area:storage-ttl, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add an extend_contract_lifetime entrypoint for participant-funded storage TTL top-ups

### Description
`ttl::extend_contract_and_milestones_ttl` and `ttl::extend_contract_ttl` are only invoked as side effects of mutating calls, so a long-idle escrow can drift toward eviction with no way for participants to intervene. Expose a public entrypoint that lets any contract participant proactively refresh persistent storage TTL without changing escrow state.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `extend_contract_lifetime(env, contract_id: u32, caller: Address) -> bool` calling `caller.require_auth()` and delegating to `ttl::extend_contract_and_milestones_ttl`.
- Restrict callers to the contract's client, freelancer, or arbiter; reject unknown contract ids via the same path as `contract_exists`.
- The call must be a pure TTL operation: no status transition, no accounting mutation.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-extend-contract-lifetime`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/ttl_tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(escrow): add extend_contract_lifetime ttl top-up entrypoint`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add a get_contract_participants reader returning client, freelancer, and arbiter together"
labels: type:feature, area:read-api, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add a get_contract_participants reader returning client, freelancer, and arbiter together

### Description
Front-ends currently call `get_contract` and decode the full `Contract` struct just to render the three party addresses, which pulls in milestone accounting and status fields they do not need. A narrow participants reader keeps the ABI surface for role checks stable even as `Contract` gains fields.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a `ContractParticipants` struct (`client`, `freelancer`, `arbiter: Option<Address>`) and `get_contract_participants(env, contract_id: u32) -> ContractParticipants`.
- Reuse the not-found semantics already applied in `get_contract` rather than panicking with a bare storage miss.
- Extend TTL on read the same way other persistent readers do via `ttl::extend_contract_ttl`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-get-contract-participants`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/summary.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(escrow): add get_contract_participants read entrypoint`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add a get_approval_deadline reader exposing ledgers remaining before approval expiry"
labels: type:feature, area:approvals, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add a get_approval_deadline reader exposing ledgers remaining before approval expiry

### Description
`approve_milestone_release` writes approvals into temporary storage with a bounded TTL, and `get_milestone_approvals` returns only who approved. Callers cannot tell how long an approval will remain valid, so a `MultiSig` release can silently fail when one approval evicts before the second arrives.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `get_approval_deadline(env, contract_id: u32, milestone_index: u32) -> Option<u32>` returning ledgers remaining, computed against `ttl::compute_expiry`.
- Return `None` when no live approval exists, distinguishing "never approved" from "approved and evicted".
- Do not mutate TTL from this reader; keep it a pure view.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-get-approval-deadline`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/approval_expiry.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(escrow): add get_approval_deadline reader for approval ttl`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Reject role-overlapping addresses in propose_client_migration"
labels: type:enhancement, area:migration, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Harden propose_client_migration against role-overlapping addresses

### Description
`propose_client_migration` stores a `PendingClientMigration` for an arbitrary new client address without checking it against the contract's other roles. Accepting a migration to the freelancer or arbiter address collapses two independent parties into one, defeating the release authorization and dispute models.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Validate in `propose_client_migration` that the proposed client is not the current client, the freelancer, or the arbiter, and not the escrow contract's own address.
- Emit a distinct `EscrowError` variant for the overlap case rather than a generic invalid-input error.
- Re-check the same invariant in `accept_client_migration`, since roles can change between proposal and acceptance.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-migration-role-overlap`
- **Write code in:** `contracts/escrow/src/migration.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/client_migration.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`fix(escrow): reject role-overlapping client in migration proposal`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Return concrete defaults from get_governed_parameters instead of None"
labels: type:enhancement, area:governance, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Return concrete defaults from get_governed_parameters instead of None

### Description
`get_governed_parameters` returns `Option<GovernedParameters>` and yields `None` until `set_governed_params` has been called, while the enforcement paths still apply hardcoded fallbacks. Integrators therefore see `None` even though limits are actively enforced, which misrepresents on-chain behavior.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Change `get_governed_parameters` to return a `GovernedParameters` populated from the same default constants the enforcement code uses when storage is empty.
- Keep a separate boolean or timestamp reader so callers can still tell whether governance has explicitly written parameters.
- Ensure `set_protocol_fee_bps` and `set_governed_params` writes remain the single source of truth once present.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-governed-params-defaults`
- **Write code in:** `contracts/escrow/src/governance.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/governance.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`refactor(escrow): return default governed parameters when unset`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add validate_deposit_amount tests for exact, under, and over funding boundaries"
labels: type:test, area:amount-validation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add validate_deposit_amount tests for exact, under, and over funding boundaries

### Description
`amount_validation::validate_deposit_amount` decides whether a deposit is acceptable against the contract total and current funded amount, but has no direct unit coverage at its decision boundaries. Its behavior at exactly-remaining, one-stroop-short, and one-stroop-over inputs is the difference between a stuck escrow and an overfunded one.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add table-driven tests for `validate_deposit_amount` covering zero, negative, exactly-remaining, under, and over-total amounts.
- Assert the exact `EscrowError` variant returned per case, not just that an error occurred.
- Include a case where `funded_amount` already equals the contract total so any further deposit is rejected.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-validate-deposit-amount-bounds`
- **Write code in:** `contracts/escrow/src/amount_validation.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/input_sanitization_amounts.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`test(escrow): cover validate_deposit_amount boundary cases`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add raise_dispute tests for non-participant callers and unfunded contracts"
labels: type:test, area:disputes, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add raise_dispute tests for non-participant callers and unfunded contracts

### Description
`raise_dispute` moves an escrow into `ContractStatus::Disputed`, which blocks releases and refunds until an arbiter acts. Existing coverage focuses on resolution payouts, leaving the entry gate untested: who may raise a dispute, and from which statuses.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Assert that a random address that is neither client, freelancer, nor arbiter cannot call `raise_dispute`.
- Assert rejection when the contract is still unfunded and when it has already been finalized or cancelled.
- Assert that the contract status after a successful call is exactly `Disputed` and that `get_contract` reflects it.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-raise-dispute-gates`
- **Write code in:** `contracts/escrow/src/dispute.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/dispute.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`test(escrow): cover raise_dispute caller and status gates`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add pause and emergency interaction tests for governance setters"
labels: type:test, area:pause-controls, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add pause and emergency interaction tests for governance setters

### Description
`pause`, `activate_emergency_pause`, and `resolve_emergency` set flags read by the mutating lifecycle entrypoints, but the governance surface (`set_protocol_fee_bps`, `set_governed_params`, `bind_settlement_token`) has no tests describing whether it stays reachable while paused. The intended behavior needs to be pinned down before mainnet.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a matrix of tests across `{normal, paused, emergency}` states asserting the outcome of each governance setter.
- Assert that `is_paused` and `is_emergency` report independently and that `resolve_emergency` does not implicitly unpause.
- Document the intended matrix alongside the tests so it becomes the reference behavior.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-pause-governance-matrix`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/pause_controls.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`test(escrow): add pause and emergency matrix for governance setters`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add a per-withdrawal cap and cooldown to withdraw_protocol_fees"
labels: type:security, area:protocol-fees, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add a per-withdrawal cap and cooldown to withdraw_protocol_fees

### Description
`withdraw_protocol_fees` lets the admin drain the entire balance reported by `get_accumulated_protocol_fees` in a single call. A compromised admin key converts directly into full treasury loss with no window for `activate_emergency_pause` to intervene.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a governed maximum withdrawal fraction plus a minimum ledger interval between successful withdrawals, stored under a dedicated `DataKey`.
- Reject calls that exceed the cap or land inside the cooldown with explicit `EscrowError` variants, and record the last withdrawal ledger.
- Keep the accumulated-fee accounting exact: partial withdrawal must decrement by exactly the transferred amount.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-fee-withdrawal-rate-limit`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/protocol_fees.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(escrow): rate limit protocol fee withdrawals`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Require a bound settlement token before create_contract accepts new escrows"
labels: type:security, area:settlement-token, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Require a bound settlement token before create_contract accepts new escrows

### Description
`create_contract` succeeds even when `is_settlement_token_bound` returns false, producing escrows that can never move real funds through `deposit_funds` or `release_milestone`. Worse, an admin can later `bind_settlement_token` to a different asset than participants assumed when they agreed to the milestone amounts.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Reject `create_contract` when no settlement token is bound, using a dedicated `EscrowError` variant.
- Persist the bound token address on the `Contract` record at creation so later rebinding cannot retroactively change an existing escrow's asset.
- Have `deposit_funds` and `release_milestone` settle against the per-contract token rather than re-reading global state.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-require-settlement-token`
- **Write code in:** `contracts/escrow/src/create_contract.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/sac_custody.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`fix(escrow): require bound settlement token at contract creation`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Gate submit_work_evidence to the freelancer and pre-release milestone states"
labels: type:security, area:work-evidence, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Gate submit_work_evidence to the freelancer and pre-release milestone states

### Description
`submit_work_evidence` writes into `Milestone.work_evidence` and is read back by `get_work_evidence`, but the caller and milestone-state gates are not tight enough. Evidence written after a milestone has been released or refunded rewrites the audit trail for an already-settled payment.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Restrict the caller to the contract's freelancer with `require_auth`, rejecting client, arbiter, and third parties.
- Reject submissions when the milestone is already released or refunded, or when the contract is `Cancelled`, `Disputed`, or finalized.
- Reject empty evidence strings and keep the existing upper length bound enforced on the same path.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-work-evidence-gating`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/access_control.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`fix(escrow): restrict submit_work_evidence caller and milestone state`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Document the now_seconds ledger-time source and deterministic time control in tests"
labels: type:docs, area:ledger-time, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Document the now_seconds ledger-time source and deterministic time control in tests

### Description
`utils::now_seconds` is the single time source behind `is_milestone_overdue`, migration expiry, and the admin timelock, yet its semantics are only described in a stale commented-out example in `utils.rs`. Contributors do not know that ledger timestamps are validator-influenced and must never be used for fine-grained deadlines.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Write a docs page covering `now_seconds`, its precision and trust assumptions, and every call site that depends on it.
- Show how to advance time deterministically in tests via the Soroban test ledger info, with a worked example matching `test/timeout_tests.rs`.
- Remove or refresh the outdated commented `check_timeout` sketch in `utils.rs` so code and docs agree.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-ledger-time-source`
- **Write code in:** `contracts/escrow/src/utils.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/timeout_tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`docs(escrow): document now_seconds ledger time source`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Add a WASM upgrade and redeploy runbook for the escrow contract"
labels: type:docs, area:operations, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add a WASM upgrade and redeploy runbook for the escrow contract

### Description
The repository documents deployment and mainnet readiness, but not the operational sequence for shipping a new WASM to an already-live escrow with in-flight contracts. Operators need a concrete order of operations spanning `activate_emergency_pause`, the upgrade itself, and post-upgrade verification via `get_mainnet_readiness_info`.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Write `docs/escrow/upgrade-runbook.md` covering pre-upgrade checks, pause, WASM install and upgrade commands, unpause, and rollback.
- Include a post-upgrade verification checklist asserting `get_admin`, `get_settlement_token`, `get_protocol_fee_bps`, and `get_next_contract_id` are unchanged.
- State explicitly which storage layout changes require a migration step versus a plain code swap.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-upgrade-runbook`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/mainnet_readiness.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`docs(escrow): add wasm upgrade and redeploy runbook`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Give get_bounds a dedicated ContractBounds return type instead of reusing ContractSummary"
labels: type:refactor, area:read-api, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Give get_bounds a dedicated ContractBounds return type instead of reusing ContractSummary

### Description
`get_bounds` returns a `ContractSummary`, the same struct `get_contract_summary` uses to describe a single escrow. Overloading the indexer summary type to carry protocol-wide limits forces callers to ignore meaningless per-contract fields and couples the limits ABI to the summary schema version.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Introduce a `ContractBounds` struct holding only the real limits (max milestones, max single amount, max escrow total, fee bps ceiling).
- Change `get_bounds` to return `ContractBounds` and stop populating unrelated `ContractSummary` fields with placeholders.
- Keep `ContractSummary` reserved for `get_contract_summary` so its schema version tracks one concern only.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-contract-bounds-type`
- **Write code in:** `contracts/escrow/src/types.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/create_contract_bounds.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`refactor(escrow): return dedicated ContractBounds from get_bounds`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
++++++
---
type: Feature
title: "Extract the repeated contract load and status check preamble into a require_active_contract helper"
labels: type:refactor, area:lifecycle, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Extract the repeated contract load and status check preamble into a require_active_contract helper

### Description
`deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `cancel_contract`, `raise_dispute`, and `submit_work_evidence` each open with the same block: load the `Contract` from storage, panic if missing, then reject terminal statuses. The duplication is why guards drift between entrypoints as new statuses are added.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a `require_active_contract(env, contract_id) -> Contract` helper that loads, verifies existence, extends TTL, and rejects `Cancelled`, `Refunded`, and finalized states.
- Replace the inline preamble in every mutating entrypoint with the helper, keeping each entrypoint's additional status requirements explicit at the call site.
- Preserve the exact `EscrowError` variants currently emitted so existing negative-path tests stay green.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-require-active-contract`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/lifecycle.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`refactor(escrow): centralize contract load and status guard`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
