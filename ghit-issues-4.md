---
type: Feature
title: "Move escrowed funds on-chain via SAC token transfers in deposit_funds and release_milestone"
labels: type:feature, area:settlement, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Move escrowed funds on-chain via SAC token transfers in deposit_funds and release_milestone

### Description
The escrow in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) tracks `funded_amount`, `released_amount`, and `refunded_amount` purely as integer counters: `deposit_funds` only increments `contract.funded_amount`, and `release_milestone` only flips `milestone.released` and bumps `released_amount`. No actual value ever moves — the contract holds no balance and never calls `soroban_sdk::token`. This means the on-chain accounting and real custody can drift, and a "released" milestone never actually pays the freelancer.

This issue makes the escrow custodial: bind a configurable Stellar Asset Contract (SAC) token at `initialize`, pull funds from the client on `deposit_funds`, and push funds to the freelancer on `release_milestone`, atomically with the existing counter updates.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Store a settlement `token: Address` under a new `DataKey::SettlementToken`, set once at `initialize`.
- In `deposit_funds`, call `token::Client::transfer(&caller, &contract_address, &amount)` after `caller.require_auth()`, keeping the `funded_amount` update atomic with the transfer.
- In `release_milestone`, transfer `milestone.amount` (minus protocol fee) from the contract to `contract.freelancer` after marking it released.
- Add a typed error path for failed/under-funded transfers; keep `EscrowError` codes append-only for client-SDK stability.
- Preserve all existing invariants: pause gate, approval checks, saturating arithmetic, and TTL bumps.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-sac-onchain-custody`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — `DataKey::SettlementToken`, token binding in `initialize`, and `soroban_sdk::token::Client` transfers in `deposit_funds` / `release_milestone`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/deposit.rs`](contracts/escrow/src/test/deposit.rs) and [`contracts/escrow/src/test/release.rs`](contracts/escrow/src/test/release.rs) — register a mock SAC via `env.register_stellar_asset_contract`, asserting balance deltas, auth flows, and event payloads.
  - **Add documentation:** update [`README.md`](README.md) and [`docs/escrow/README.md`](docs/escrow/README.md) with the custody lifecycle.
  - Include NatSpec-style doc comments (`///`) on every changed entrypoint.
  - Validate security assumptions: no double-pay on repeated release, correct auth on deposit/release, overflow safety on balance math.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: zero balance, exact-balance release, paused contract, and unauthorized caller.
- Include full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`feat: move escrowed funds on-chain via SAC token transfers in deposit and release`

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
title: "Implement submit_work_evidence to populate the unused Milestone.work_evidence field"
labels: type:feature, area:milestones, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Implement submit_work_evidence to populate the unused Milestone.work_evidence field

### Description
The `Milestone` struct in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) carries a `work_evidence: Option<String>` field, but no entrypoint ever sets it — `create_contract` initializes every milestone with `work_evidence: None`, and nothing else writes to it. Freelancers have no on-chain way to attach a deliverable reference (e.g. an IPFS CID or URL hash) before a client approves and releases a milestone.

This issue adds a `submit_work_evidence(contract_id, caller, milestone_index, evidence)` entrypoint that lets the freelancer record evidence for an unreleased milestone, emitting an event for indexers.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `submit_work_evidence` that requires `caller.require_auth()` and verifies `caller == contract.freelancer`.
- Reject submission when the contract is not `Funded`, the milestone is released or refunded, or the index is out of bounds (`IndexOutOfBounds`).
- Bound the evidence `String` length to avoid storage bloat; reject oversized input with a typed error.
- Emit a `work_evidence` event with `(contract_id, milestone_index, freelancer, timestamp)` and bump milestone TTL.
- Honor the pause/emergency gate and `require_not_finalized`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-submit-work-evidence`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — new `submit_work_evidence` entrypoint mutating the milestone vector.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/release.rs`](contracts/escrow/src/test/release.rs) — assert evidence is stored, overwrite rules, and all rejection paths.
  - **Add documentation:** update [`docs/escrow/README.md`](docs/escrow/README.md) describing the evidence-before-release flow.
  - Include NatSpec-style doc comments (`///`) on the new entrypoint.
  - Validate security assumptions: only freelancer can submit, no submission after release.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: oversized evidence, wrong caller, released/refunded milestone, finalized contract.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: add submit_work_evidence entrypoint to record milestone deliverables`

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
title: "Support partial funding with the PartiallyFunded status in deposit_funds"
labels: type:feature, area:deposits, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Support partial funding with the PartiallyFunded status in deposit_funds

### Description
`ContractStatus::PartiallyFunded` is defined in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) and accepted by `cancel_contract`, but `deposit_funds` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) never sets it: a contract stays `Created` until `funded_amount >= total_amount`, at which point it jumps straight to `Funded`. A client who deposits in installments leaves the contract in a misleading `Created` state, indistinguishable from an unfunded contract.

This issue makes `deposit_funds` transition to `PartiallyFunded` when funds are present but below the milestone total.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- After incrementing `funded_amount`, set status to `PartiallyFunded` when `0 < funded_amount < total_amount`, and `Funded` when `funded_amount >= total_amount`.
- Allow `deposit_funds` to accept further deposits while in `PartiallyFunded` (not only `Created`).
- Emit a `deposit` event including the new status so indexers can distinguish partial vs full funding.
- Preserve the existing positivity check, client-only auth, and TTL bumps.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-partially-funded-status`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — the status-transition block in `deposit_funds`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/deposit.rs`](contracts/escrow/src/test/deposit.rs) — assert PartiallyFunded → Funded progression across multiple deposits.
  - **Add documentation:** update the status-machine notes in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`) on `deposit_funds`.
  - Validate security assumptions: no skipped states, no over-funding regressions.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: single full deposit, two partial deposits, exact-total deposit.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: transition to PartiallyFunded on installment deposits`

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
title: "Add freelancer acceptance step using the unused ContractStatus::Accepted state"
labels: type:feature, area:lifecycle, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add freelancer acceptance step using the unused ContractStatus::Accepted state

### Description
`ContractStatus::Accepted` is declared in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) but is never assigned anywhere in the contract. Today a contract goes `Created → Funded` with no on-chain record that the freelancer ever agreed to the terms; the client funds work that the freelancer may never have accepted.

This issue adds an `accept_contract(contract_id, freelancer)` entrypoint that records explicit freelancer consent before funds are released.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `accept_contract` requiring `freelancer.require_auth()` and `caller == contract.freelancer`.
- Allow acceptance only from `Created` or `Funded`; set status to `Accepted` (or gate releases on an `accepted` flag if status ordering must be preserved).
- Optionally require acceptance before `release_milestone` can succeed, behind a clearly documented rule.
- Emit an `accepted` event with `(contract_id, freelancer, timestamp)`.
- Honor the pause gate and `require_not_finalized`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-freelancer-acceptance`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — new `accept_contract` entrypoint and any release-gating wiring.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/release_authorization.rs`](contracts/escrow/src/test/release_authorization.rs) — assert acceptance flow and rejection of unauthorized callers.
  - **Add documentation:** update the lifecycle diagram in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: only the freelancer can accept, no acceptance after terminal states.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: double acceptance, acceptance by client, acceptance of cancelled contract.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: add accept_contract using the Accepted status for freelancer consent`

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
title: "Add revoke_approval to let a party withdraw a milestone approval before release"
labels: type:feature, area:approvals, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add revoke_approval to let a party withdraw a milestone approval before release

### Description
`approve_milestone` in [`contracts/escrow/src/approvals.rs`](contracts/escrow/src/approvals.rs) records `client_approved` / `freelancer_approved` / `arbiter_approved` flags in temporary storage, but there is no way to undo an approval. Once a party approves, the only paths are release (which clears approvals) or TTL expiry. A client who approves prematurely, or who discovers a problem before release, is stuck waiting up to seven days for the approval to expire.

This issue adds a `revoke_approval(contract_id, caller, milestone_index)` entrypoint that lets the same party clear only their own approval flag.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `revoke_approval` that requires `caller.require_auth()` and clears only the caller's flag (client/freelancer/arbiter), leaving other parties' flags intact.
- Reject revocation when no approval record exists or the milestone is already released (`MilestoneAlreadyReleased`).
- Remove the approval record entirely when all three flags become false.
- Emit a `revoked` event with `(contract_id, milestone_index, caller)`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-revoke-approval`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/approvals.rs`](contracts/escrow/src/approvals.rs) — new `revoke_approval` helper plus an entrypoint wrapper in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/release_authorization.rs`](contracts/escrow/src/test/release_authorization.rs) — assert partial revocation in MultiSig and that release fails after revoke.
  - **Add documentation:** update the approval section of [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: a party can only revoke their own flag.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: revoke without prior approval, revoke after release, MultiSig revoke-one-of-two.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: add revoke_approval to withdraw a milestone approval`

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
title: "Add batch release_milestones entrypoint for releasing multiple milestones atomically"
labels: type:feature, area:release, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add batch release_milestones entrypoint for releasing multiple milestones atomically

### Description
`release_milestone` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) releases exactly one milestone per transaction. `refund_unreleased_milestones` already accepts a `Vec<u32>` of indices, but there is no batched counterpart for releases. A client completing several milestones at once must submit one transaction per milestone, paying repeated load/store costs and risking partial completion if some succeed and some fail.

This issue adds `release_milestones(contract_id, caller, milestone_indices: Vec<u32>)` that validates all indices first, then releases them atomically.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Validate the whole batch (duplicates, bounds, already-released/refunded, sufficient balance, valid approvals per index) before any mutation, mirroring `refund_unreleased_milestones`.
- Apply protocol-fee accumulation per released milestone and clear approvals for each released index.
- Update `released_amount`, transition to `Completed` only when all milestones are terminal, and emit one event summarizing the batch.
- Reuse the existing single-release authorization checks so behavior stays consistent.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-batch-release`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — new `release_milestones` entrypoint sharing logic with `release_milestone`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/release.rs`](contracts/escrow/src/test/release.rs) — assert all-or-nothing semantics and accumulated fees.
  - **Add documentation:** note batch release in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: no partial application when one index is invalid.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: duplicate indices, one already-released index, exact-balance batch.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: add atomic batch release_milestones entrypoint`

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
title: "Add milestone deadline and timeout-based auto-refund to escrow contracts"
labels: type:feature, area:timeouts, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add milestone deadline and timeout-based auto-refund to escrow contracts

### Description
The `Milestone` struct in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) has no deadline, and the `utils::now_seconds` helper in [`contracts/escrow/src/utils.rs`](contracts/escrow/src/utils.rs) is unused outside docs. There is no mechanism for a client to reclaim funds when a freelancer stalls indefinitely — the only refund path, `refund_unreleased_milestones`, has no time precondition.

This issue introduces optional per-milestone deadlines and a `claim_timeout_refund` entrypoint that lets the client recover an unreleased milestone after its deadline passes.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add an optional `deadline: Option<u64>` field to `Milestone` and accept deadlines in `create_contract` (preserve `contracttype` layout compatibility).
- Add `claim_timeout_refund(contract_id, milestone_index)` that refunds an unreleased, undisputed milestone only when `now_seconds(env) > deadline`.
- Require `client.require_auth()`, reject released/refunded milestones, and update `refunded_amount` plus status using the existing accounting rules.
- Emit a `timeout_refund` event and honor the pause gate.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-milestone-timeouts`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) and [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) — deadline field and timeout refund logic using `utils::now_seconds`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/timeout_tests.rs`](contracts/escrow/src/test/timeout_tests.rs) — drive `env.ledger().set` to assert pre/post-deadline behavior.
  - **Add documentation:** add `docs/escrow/timeouts.md` covering deadline semantics.
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: no refund before deadline, no double refund.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: exactly-at-deadline, no-deadline milestone, already-released milestone.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: add milestone deadlines and timeout-based auto-refund`

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
title: "Expose a paginated list_contracts_by_participant indexer view"
labels: type:feature, area:indexer, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Expose a paginated list_contracts_by_participant indexer view

### Description
The contract exposes `get_contract` and `get_milestones` for single-contract lookups in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs), but offers no way to enumerate the contracts a given client or freelancer participates in. Front ends must scan every `DataKey::Contract(id)` from `1..NextContractId` client-side, which is slow and fragile.

This issue maintains a per-participant index and exposes a paginated read so a dashboard can list a user's escrows efficiently.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- On `create_contract`, append the new id to per-address index vectors under new `DataKey` variants (e.g. `ClientContracts(Address)` / `FreelancerContracts(Address)`).
- Add `list_contracts_by_participant(addr, start, limit) -> Vec<u32>` returning a bounded page of contract ids.
- Cap `limit` to avoid unbounded reads and bump index-entry TTL on write.
- Keep the index append-only and consistent with contract creation; do not change existing entrypoint signatures.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-participant-index`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — index maintenance in `create_contract` and the new paginated reader.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/persistence.rs`](contracts/escrow/src/test/persistence.rs) — assert pagination bounds and per-participant correctness.
  - **Add documentation:** document the index keys in [`docs/escrow/state-persistence.md`](docs/escrow/state-persistence.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: reads are non-mutating and limit-bounded.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: empty index, limit larger than index, out-of-range start.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: add paginated list_contracts_by_participant indexer view`

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
title: "Emit a structured milestone_released event with fee and accounting deltas"
labels: type:feature, area:events, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Emit a structured milestone_released event with fee and accounting deltas

### Description
`release_milestone` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) mutates `released_amount`, accumulates protocol fees, and may transition the contract to `Completed`, but it emits **no event at all** on success. Indexers and front ends cannot observe releases without diffing full contract state. By contrast, `create_contract` and the governance module already publish events.

This issue adds a structured `milestone_released` event so off-chain consumers can track releases directly.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Publish a `milestone_released` event keyed by `(symbol, contract_id)` carrying `(milestone_index, amount, fee, new_released_amount, caller, timestamp)`.
- Emit a separate `contract_completed` event when the release transitions status to `Completed`.
- Follow the existing `symbol_short!` topic conventions used elsewhere in the contract.
- Do not change the function's return value or control flow other than adding the publish calls.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-release-events`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — `env.events().publish` calls at the end of `release_milestone`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/release.rs`](contracts/escrow/src/test/release.rs) — assert event topics and payloads via `env.events().all()`.
  - **Add documentation:** add the event schema to [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`) describing emitted events.
  - Validate security assumptions: events never leak secrets and are emitted only on success.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: zero-fee release, final-milestone completion event.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: emit structured milestone_released and contract_completed events`

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
title: "Emit a cancelled event from cancel_contract for indexer observability"
labels: type:feature, area:events, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Emit a cancelled event from cancel_contract for indexer observability

### Description
`cancel_contract` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) sets `status = Cancelled` and persists the contract, but emits no event. There is no on-chain signal that a contract was cancelled, so indexers must poll and diff to detect cancellations — inconsistent with `create_contract` and finalize, which both publish events.

This issue adds a `cancelled` event carrying who cancelled and when.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Publish a `cancelled` event keyed by `(symbol, contract_id)` with `(caller, previous_status, timestamp)`.
- Emit only after the state write succeeds, following existing topic conventions.
- Do not alter authorization, allowed-status checks, or the return value.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-cancel-event`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — `env.events().publish` in `cancel_contract`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/pause_controls.rs`](contracts/escrow/src/test/pause_controls.rs) — assert the cancelled event is emitted with the right payload.
  - **Add documentation:** list the event in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: event emitted only on a successful cancel.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: cancel from Created, Funded, and PartiallyFunded.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: emit cancelled event from cancel_contract`

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
title: "Add cancel_client_migration to let the current client withdraw a pending migration"
labels: type:feature, area:migration, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add cancel_client_migration to let the current client withdraw a pending migration

### Description
[`contracts/escrow/src/migration.rs`](contracts/escrow/src/migration.rs) implements `propose_client_migration` and `accept_client_migration`, but there is no way to cancel a proposal once made. A current client who proposed the wrong `new_client`, or who changed their mind, must wait for the 21-day `PENDING_MIGRATION_TTL_LEDGERS` to expire before they can propose again — and `propose_client_migration` panics with `InvalidState` if a pending proposal still exists.

This issue adds `cancel_client_migration` so the current client can revoke a live proposal immediately.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `cancel_client_migration(contract_id, current_client)` requiring `current_client.require_auth()` and `current_client == contract.client`.
- Reject when no live pending migration exists (`InvalidState`) and honor the pause gate / `require_not_finalized`.
- Remove the transient pending-migration entry via `remove_transient` and emit a `client_migration_cancelled` event.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-cancel-client-migration`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/migration.rs`](contracts/escrow/src/migration.rs) — new `cancel_client_migration` entrypoint.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/client_migration.rs`](contracts/escrow/src/test/client_migration.rs) — assert cancel clears the proposal and re-propose then succeeds.
  - **Add documentation:** update the migration flow in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: only the current client can cancel.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: cancel with no proposal, cancel then re-propose, cancel by non-client.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: add cancel_client_migration to revoke a pending proposal`

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
title: "Add cancel_governance_admin_proposal to abort a pending two-step admin transfer"
labels: type:feature, area:governance, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add cancel_governance_admin_proposal to abort a pending two-step admin transfer

### Description
[`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs) implements `propose_governance_admin` and `accept_governance_admin` storing a `DataKey::PendingAdmin`, but there is no way for the current admin to cancel a proposal. If the wrong address was proposed, the pending admin can still accept and seize control until the proposal is overwritten by another propose call — there is no explicit revocation.

This issue adds `cancel_governance_admin_proposal` so the current admin can clear `PendingAdmin` immediately.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `cancel_governance_admin_proposal` requiring the stored `Admin` to `require_auth()` and the contract to be initialized.
- Reject when no `PendingAdmin` exists (`InvalidState`).
- Remove `DataKey::PendingAdmin` and emit an `(admin, "cancelled")` audit event with `(admin, cancelled_proposal, timestamp)`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-cancel-admin-proposal`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs) — new `cancel_governance_admin_proposal` entrypoint.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/security.rs`](contracts/escrow/src/test/security.rs) — assert cancel blocks a later accept and that only admin can cancel.
  - **Add documentation:** update the governance section in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: pending admin cannot accept after cancellation.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: cancel without a proposal, cancel by non-admin, accept-after-cancel rejected.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: add cancel_governance_admin_proposal to abort a pending transfer`

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
title: "Persist reputation_issued in the finalization ContractSummary snapshot"
labels: type:feature, area:finalization, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Persist reputation_issued in the finalization ContractSummary snapshot

### Description
`summarize_contract` in [`contracts/escrow/src/finalize.rs`](contracts/escrow/src/finalize.rs) hardcodes `reputation_issued: false` in the `ContractSummary` it builds, even though the contract already tracks `DataKey::ReputationIssued(contract_id)`. The immutable close record written by `finalize_contract` therefore always claims reputation was not issued, which is wrong for any completed contract that received a rating.

This issue makes the snapshot read the real reputation-issued flag.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- In `summarize_contract`, read `DataKey::ReputationIssued(contract_id)` and set `reputation_issued` accordingly instead of the hardcoded `false`.
- Keep the `CONTRACT_SUMMARY_SCHEMA_VERSION` unchanged unless the field semantics change; document the fix.
- Ensure `get_contract_summary` (the indexer view) and `finalize_contract` both reflect the corrected value.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-summary-reputation-flag`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/finalize.rs`](contracts/escrow/src/finalize.rs) — fix `summarize_contract`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/reputation.rs`](contracts/escrow/src/test/reputation.rs) — assert the flag is true after `issue_reputation` then finalize.
  - **Add documentation:** note the field semantics in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: the snapshot is read-only and consistent.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: finalize without reputation, finalize after reputation.
- Include full `cargo test` output in the PR description.

### Example commit message
`fix: read real reputation_issued flag in ContractSummary snapshot`

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
title: "Apply the pause and emergency gate to deposit_funds, release_milestone, and refunds"
labels: type:security, area:pause, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Apply the pause and emergency gate to deposit_funds, release_milestone, and refunds

### Description
The README claims that "when paused, all mutating escrow operations (`create_contract`, `deposit_funds`, `release_milestone`, `issue_reputation`, `cancel_contract`) are blocked with `ContractPaused`." But the `require_not_paused` helper in [`contracts/escrow/src/finalize.rs`](contracts/escrow/src/finalize.rs) is only invoked by finalize and migration. `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `create_contract`, `cancel_contract`, and `issue_reputation` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) never call it — so funds can move while the contract is paused or in an emergency.

This issue closes that gap so the pause switch actually halts value-moving operations.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Call `Self::require_not_paused(&env)` at the top of every mutating escrow entrypoint named in the README before any state read/write.
- Ensure both `Paused` and `Emergency` flags are honored (the helper already checks both).
- Keep read-only queries unblocked.
- Verify the documented behavior is now enforced and update the README only if its list changes.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-enforce-pause-gate`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add `require_not_paused` guards to the mutating entrypoints.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/pause_controls.rs`](contracts/escrow/src/test/pause_controls.rs) — assert each entrypoint panics with `ContractPaused` while paused and in emergency.
  - **Add documentation:** confirm the pause matrix in [`docs/escrow/emergency-controls.md`](docs/escrow/emergency-controls.md).
  - Include NatSpec-style doc comments (`///`) noting the pause precondition.
  - Validate security assumptions: no value movement while paused.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: paused-only, emergency-only, and unpause-then-succeed for each entrypoint.
- Include full `cargo test` output and a security notes section in the PR description.

### Example commit message
`fix: enforce pause/emergency gate on all mutating escrow entrypoints`

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
title: "Use saturating or checked arithmetic for funded_amount and released_amount mutations"
labels: type:security, area:accounting, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Use saturating or checked arithmetic for funded_amount and released_amount mutations

### Description
`deposit_funds` does `contract.funded_amount += amount`, `release_milestone` does `contract.released_amount += milestone.amount`, and `refund_unreleased_milestones` does `contract.refunded_amount += total_refund_amount` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — all plain `i128` additions. The codebase already provides `safe_add_amounts` / `safe_subtract_amounts` in [`contracts/escrow/src/amount_validation.rs`](contracts/escrow/src/amount_validation.rs), but these hot paths don't use them. A crafted sequence of large deposits could overflow and panic ungracefully or wrap in release builds.

This issue routes all accounting mutations through the existing checked helpers with a typed error.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Replace the raw `+=` on `funded_amount`, `released_amount`, and `refunded_amount` with `safe_add_amounts`, panicking with a typed overflow error on failure.
- Use `safe_subtract_amounts` for the `available_balance` computations to detect accounting-invariant violations.
- Keep error codes append-only and preserve all existing checks (positivity, state, auth).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-checked-accounting`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — swap raw arithmetic for the helpers in `deposit_funds`, `release_milestone`, and `refund_unreleased_milestones`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/input_sanitization_amounts.rs`](contracts/escrow/src/test/input_sanitization_amounts.rs) — drive near-`i128::MAX` deposits to assert a clean typed failure.
  - **Add documentation:** note the overflow policy in [`docs/escrow/SECURITY.md`](docs/escrow/SECURITY.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: no silent wraparound, deterministic failure.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: overflow on deposit, overflow on release sum, invariant violation on subtract.
- Include full `cargo test` output and a security notes section in the PR description.

### Example commit message
`fix: use checked arithmetic for escrow accounting mutations`

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
title: "Block deposits and migrations on cancelled contracts in deposit_funds"
labels: type:security, area:lifecycle, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Block deposits and migrations on cancelled contracts in deposit_funds

### Description
`cancel_contract` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) moves a contract to `Cancelled`, but `deposit_funds` only checks `status == Created` to allow deposits — and `create_contract` never produces a `Cancelled` contract, so the guard is fine there. The real risk is elsewhere: once on-chain custody lands, a `Cancelled` contract must categorically reject any further value movement, and the current `InvalidState` message does not distinguish "cancelled" from "already funded," making audits harder.

This issue tightens lifecycle enforcement so a cancelled contract is a hard terminal state for all value-moving operations.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add explicit `Cancelled` / `Refunded` rejection in `deposit_funds`, `release_milestone`, and `refund_unreleased_milestones` with a distinct, descriptive error rather than a generic `InvalidState`.
- Confirm `cancel_contract` cannot run on `Completed`, `Disputed`, `Refunded`, or already-`Cancelled` contracts (it currently allows only `Created`/`PartiallyFunded`/`Funded`).
- Keep error codes append-only and document the terminal-state matrix.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-terminal-state-guards`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — terminal-state guards in the value-moving entrypoints.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/security.rs`](contracts/escrow/src/test/security.rs) — assert cancelled/refunded contracts reject deposits, releases, and refunds.
  - **Add documentation:** add a terminal-state matrix to [`docs/escrow/SECURITY.md`](docs/escrow/SECURITY.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: no operations on terminal contracts.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: deposit after cancel, release after cancel, refund after refund.
- Include full `cargo test` output and a security notes section in the PR description.

### Example commit message
`fix: reject value-moving operations on cancelled and refunded contracts`

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
title: "Validate the aggregate milestone total against overflow in create_contract"
labels: type:security, area:create-contract, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Validate the aggregate milestone total against overflow in create_contract

### Description
`create_contract` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) validates that each milestone amount is `> 0` but never validates the **sum**. Later, `deposit_funds` computes `let total_amount: i128 = milestones.iter().map(|m| m.amount).sum();` with a plain `.sum()`, which panics on overflow. A contract created with many large milestones can therefore be funded only via a transaction that panics, effectively bricking it, and the overflow surfaces far from where the bad input was accepted.

This issue validates the milestone total at creation time using the checked helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- In `create_contract`, accumulate the milestone total with `safe_add_amounts` and reject with a typed error if it overflows or exceeds a configured maximum.
- Replace the raw `.sum()` in `deposit_funds` with the same checked accumulation.
- Enforce a maximum milestone count to bound iteration cost.
- Keep error codes append-only and document the new validation.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-validate-milestone-total`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — checked total in `create_contract` and `deposit_funds`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/create_contract_bounds.rs`](contracts/escrow/src/test/create_contract_bounds.rs) — assert overflowing totals are rejected at creation.
  - **Add documentation:** note the total bound in [`docs/escrow/SECURITY.md`](docs/escrow/SECURITY.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: no panic path reachable via funding.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: two near-max milestones, max-count milestones, exact-max total.
- Include full `cargo test` output and a security notes section in the PR description.

### Example commit message
`fix: validate aggregate milestone total against overflow in create_contract`

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
title: "Fix the negative pending-reputation-credit underflow in issue_reputation"
labels: type:security, area:reputation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Fix the negative pending-reputation-credit underflow in issue_reputation

### Description
`issue_reputation` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) decrements pending credits with `env.storage().persistent().set(&pending_key, &(pending - 1));` where `pending` defaults to `0` when no credit was ever granted. Because nothing in the contract ever **increments** `PendingReputationCredits`, every successful `issue_reputation` writes a negative balance (e.g. `-1`), and `get_pending_reputation_credits` then returns nonsense negative numbers. The credit accounting is silently broken.

This issue corrects the pending-credit lifecycle so the counter never goes negative.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Either guard the decrement so it never drops below zero, or introduce the missing increment (e.g. on contract completion) so the debit has a matching credit.
- Decide and document the intended semantics of `PendingReputationCredits` and make `issue_reputation` consistent with it.
- Preserve the once-per-contract `ReputationIssued` guard and all existing authorization/state checks.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-reputation-credit-underflow`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — fix the pending-credit math in `issue_reputation`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/reputation.rs`](contracts/escrow/src/test/reputation.rs) — assert credits never go negative across multiple contracts.
  - **Add documentation:** describe credit semantics in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: no negative balances, no double-issue.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: first issuance, repeated issuance attempt, multi-contract freelancer.
- Include full `cargo test` output and a security notes section in the PR description.

### Example commit message
`fix: prevent negative pending reputation credits in issue_reputation`

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
title: "Require initialization before create_contract and deposit_funds to bind the admin"
labels: type:security, area:initialization, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Require initialization before create_contract and deposit_funds to bind the admin

### Description
`create_contract`, `deposit_funds`, and `release_milestone` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) do not call `require_initialized`. Protocol-fee accumulation in `release_milestone` is even guarded by `if Self::is_initialized(&env)`, so an uninitialized contract can take deposits and release funds with **no admin, no pause authority, and no fee accounting**. The pause and emergency controls that protect users only exist once `initialize` has run, but the core money flow doesn't require it.

This issue requires initialization before any escrow lifecycle operation, so the admin-controlled safety rails always apply.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Call `Self::require_initialized(&env)` at the top of `create_contract`, `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, and `cancel_contract`.
- Keep `initialize` itself single-use and idempotent-guarded as today.
- Update the README, which implies these flows are always protected, to match enforced behavior.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-require-init`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add `require_initialized` guards.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/mainnet_readiness.rs`](contracts/escrow/src/test/mainnet_readiness.rs) — assert uninitialized calls panic with `NotInitialized`.
  - **Add documentation:** update [`README.md`](README.md) initialization requirements.
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: no money flow before admin binding.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: each entrypoint pre-init, then post-init success.
- Include full `cargo test` output and a security notes section in the PR description.

### Example commit message
`fix: require initialization before escrow lifecycle operations`

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
title: "Round protocol fees deterministically and cap them below the milestone amount"
labels: type:security, area:protocol-fees, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Round protocol fees deterministically and cap them below the milestone amount

### Description
`calculate_protocol_fee` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) computes `amount * fee_bps as i128 / 10_000`. The intermediate `amount * fee_bps` can overflow `i128` for very large amounts, the integer division silently truncates toward zero (no documented rounding policy), and `set_protocol_fee_bps` in [`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs) accepts any `u32` — including values `>= 10_000`, which would make the fee equal or exceed the released amount.

This issue makes fee computation overflow-safe, bounded, and explicitly rounded.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Use `checked_mul` / `checked_div` (or a 256-bit widening) in `calculate_protocol_fee` and panic with a typed overflow error on failure.
- Reject `set_protocol_fee_bps` values above a sane maximum (e.g. `< 10_000`) with a typed error.
- Document the rounding direction and guarantee `fee <= milestone.amount`.
- Preserve append-only error codes and existing admin authorization.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-fee-rounding-bounds`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) and [`contracts/escrow/src/governance.rs`](contracts/escrow/src/governance.rs) — safe fee math and bps bounds.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/protocol_fees.rs`](contracts/escrow/src/test/protocol_fees.rs) — assert rounding, overflow rejection, and bps bound rejection.
  - **Add documentation:** describe the fee model in [`docs/escrow/SECURITY.md`](docs/escrow/SECURITY.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: fee never exceeds the amount, no overflow.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: max amount with nonzero bps, bps at boundary, zero bps.
- Include full `cargo test` output and a security notes section in the PR description.

### Example commit message
`fix: bound and overflow-protect protocol fee calculation`

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
title: "Remove the duplicate Error and Contract definitions across lib.rs and types.rs"
labels: type:refactor, area:error-types, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Remove the duplicate Error and Contract definitions across lib.rs and types.rs

### Description
[`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) defines `enum Error` **twice** (lines ~3–29 and ~95–122) with conflicting discriminants, plus a second `Contract`, `ReleaseAuthorization`, and `MilestoneApprovals`. Meanwhile [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) defines a third error enum, `EscrowError`, whose codes (e.g. `AlreadyReleased = 9`) disagree with `types::Error::AlreadyReleased = 4`. Two parallel error taxonomies with mismatched numeric codes make client-SDK error handling ambiguous and invite silent regressions.

This issue consolidates to a single canonical error enum and removes the shadow type definitions.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Pick one canonical error enum (preferably the `types::Error` exported via `pub use`), delete the duplicate definition, and migrate `EscrowError` references to it (or vice versa) without changing wire codes for already-shipped variants.
- Remove the duplicate `Contract` / `ReleaseAuthorization` / `MilestoneApprovals` definitions, keeping one source of truth.
- Keep all discriminants append-only and document the final code catalog.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-dedup-error-types`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) and [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — unify error/type definitions.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/security.rs`](contracts/escrow/src/test/security.rs) — assert stable error codes for key failure paths.
  - **Add documentation:** update the error catalog in [`docs/escrow/SECURITY.md`](docs/escrow/SECURITY.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: no code reassignment for shipped variants.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: each previously-distinct error path still returns the documented code.
- Include full `cargo test` output in the PR description.

### Example commit message
`refactor: consolidate duplicate Error and Contract type definitions`

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
title: "Remove the duplicate next_contract_id call in create_contract"
labels: type:refactor, area:create-contract, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Remove the duplicate next_contract_id call in create_contract

### Description
`create_contract` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) calls `Self::next_contract_id(&env)` twice in a row — `let id = Self::next_contract_id(&env);` then `ttl::extend_next_contract_id_ttl(&env);` then `let id = Self::next_contract_id(&env);` again. The first binding is immediately shadowed and the collision check runs twice for no reason. There is also an unused `bump_next_contract_id` helper marked `#[allow(dead_code)]` that duplicates the inline `id + 1` write.

This issue cleans up the id-allocation path to a single call and removes the dead helper.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Collapse the duplicated `next_contract_id` calls into one, keeping the TTL bump and collision check.
- Either wire `bump_next_contract_id` into the write path or delete it, removing the `#[allow(dead_code)]`.
- Preserve overflow protection (`ContractIdOverflow`) and collision protection (`ContractIdCollision`).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-single-id-alloc`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — dedupe id allocation in `create_contract`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/contract_id_allocation.rs`](contracts/escrow/src/test/contract_id_allocation.rs) — assert sequential, gap-free, collision-safe ids.
  - **Add documentation:** note id-allocation invariants in [`docs/escrow/state-persistence.md`](docs/escrow/state-persistence.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: no reuse, no skipped ids.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: many sequential creates, collision attempt.
- Include full `cargo test` output in the PR description.

### Example commit message
`refactor: remove duplicate next_contract_id call in create_contract`

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
title: "Extract the repeated milestones-vector load/store into a single helper"
labels: type:refactor, area:storage, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Extract the repeated milestones-vector load/store into a single helper

### Description
The pattern `let milestone_key = Symbol::new(&env, "milestones"); env.storage().persistent().get(&(DataKey::Contract(contract_id), milestone_key))` is hand-rolled in at least five places across [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) (`deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `get_milestones`) and again in [`contracts/escrow/src/finalize.rs`](contracts/escrow/src/finalize.rs) and [`contracts/escrow/src/approvals.rs`](contracts/escrow/src/approvals.rs). Each site re-derives the composite key, unwraps inconsistently (`unwrap()` vs `ok_or`), and bumps TTL separately, which is error-prone.

This issue centralizes milestone-vector access behind `load_milestones` / `store_milestones` helpers.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `load_milestones(env, contract_id) -> Vec<Milestone>` and `store_milestones(env, contract_id, &Vec<Milestone>)` that build the composite key once and bump TTL consistently.
- Replace all open-coded milestone reads/writes with the helpers, normalizing error handling to a single not-found path.
- No behavioral change to entrypoints; purely a refactor with identical externally observable behavior.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-milestone-accessors`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — helpers plus call-site replacement across modules.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/persistence.rs`](contracts/escrow/src/test/persistence.rs) — assert load/store round-trips and TTL bumps.
  - **Add documentation:** note the helpers in [`docs/escrow/state-persistence.md`](docs/escrow/state-persistence.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: identical behavior, no missed TTL bumps.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: missing milestone vector, empty vector, large vector.
- Include full `cargo test` output in the PR description.

### Example commit message
`refactor: centralize milestone vector load/store helpers`

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
title: "Remove the unused MilestoneReleased DataKey variant or back it with storage"
labels: type:refactor, area:storage, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Remove the unused MilestoneReleased DataKey variant or back it with storage

### Description
`DataKey::MilestoneReleased(u32, u32)` is declared in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) but is never read or written anywhere in the contract — release status is tracked solely on the `Milestone.released` boolean inside the milestones vector. The dangling variant suggests a per-milestone release-flag store that was never wired up, which is confusing for reviewers and for anyone reasoning about the storage layout.

This issue either removes the dead variant or actually backs milestone-release state with it.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Decide the canonical source of truth for release state (the `Milestone.released` flag vs a dedicated key) and document it.
- If removing: delete `MilestoneReleased` from `DataKey` and confirm no client SDK references it.
- If keeping: write/read it in `release_milestone` consistently and add a `is_milestone_released(contract_id, index)` reader.
- Keep the `DataKey` enum layout append-only-safe for any persisted entries.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-milestone-released-key`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) and [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/release.rs`](contracts/escrow/src/test/release.rs) — assert release state is consistent post-change.
  - **Add documentation:** update the storage-key list in [`docs/escrow/state-persistence.md`](docs/escrow/state-persistence.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: single source of truth for release state.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: released vs unreleased reads after the change.
- Include full `cargo test` output in the PR description.

### Example commit message
`refactor: resolve the unused MilestoneReleased DataKey variant`

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
title: "Consolidate the orphaned deposit, release, and refund modules into the active contract"
labels: type:refactor, area:module-layout, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Consolidate the orphaned deposit, release, and refund modules into the active contract

### Description
The escrow crate ships standalone `deposit.rs`, `release.rs`, `refund.rs`, `refund_impl.rs`, and `create_contract.rs` files under `contracts/escrow/src/`, but [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) only declares `mod approvals; mod finalize; mod governance; mod ttl; mod types;` plus `dispute`/`migration`. The deposit/release/refund logic that the contract actually runs is inlined in `lib.rs`, while these parallel files are dead duplicates (e.g. `refund_impl.rs` reimplements `refund_unreleased_milestones`). This is a maintenance hazard: fixes land in one copy and not the other.

This issue eliminates the duplication by either wiring these modules in as the single implementation or deleting them.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Audit `deposit.rs`, `release.rs`, `refund.rs`, `refund_impl.rs`, and `create_contract.rs` against the inline `lib.rs` implementations.
- For each, either promote the module to the canonical implementation (and `mod`-declare it) or remove the dead file.
- Ensure exactly one implementation of each entrypoint remains, with no behavioral change.
- Confirm the build has no orphaned `mod` warnings and clippy passes.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-consolidate-orphan-modules`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — module declarations and removal of duplicate logic.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/flows.rs`](contracts/escrow/src/test/flows.rs) — assert the consolidated paths behave identically.
  - **Add documentation:** document the final module layout in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: single implementation per entrypoint.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: deposit/release/refund happy paths and failures post-consolidation.
- Include full `cargo test` output in the PR description.

### Example commit message
`refactor: consolidate orphaned deposit/release/refund modules`

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
title: "Wire amount_validation constants into a single MAX_SINGLE_AMOUNT enforcement path"
labels: type:refactor, area:validation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Wire amount_validation constants into a single MAX_SINGLE_AMOUNT enforcement path

### Description
[`contracts/escrow/src/amount_validation.rs`](contracts/escrow/src/amount_validation.rs) defines `MAX_SINGLE_AMOUNT_STROOPS`, `MIN_POSITIVE_AMOUNT`, `STROOP_PRECISION`, and `validate_single_amount`, all marked `#[allow(dead_code)]` with comments noting they are "available for callers; not used internally." Meanwhile `create_contract` and `deposit_funds` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) hand-roll their own `amount <= 0` checks and enforce no upper bound, so a single milestone can be `i128::MAX`.

This issue wires `validate_single_amount` into the real entrypoints so the module's bounds are actually enforced.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Call `validate_single_amount` for each milestone amount in `create_contract` and for the `amount` in `deposit_funds`, mapping `AmountValidationError` to the canonical contract error.
- Remove the now-redundant inline `amount <= 0` checks.
- Drop the `#[allow(dead_code)]` attributes once the items are used.
- Keep behavior backward-compatible for amounts within bounds.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-wire-amount-validation`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — invoke `amount_validation` helpers.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/input_sanitization_amounts.rs`](contracts/escrow/src/test/input_sanitization_amounts.rs) — assert max-bound and min-bound enforcement.
  - **Add documentation:** describe the validation bounds in [`docs/escrow/SECURITY.md`](docs/escrow/SECURITY.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: no amount above the max is accepted.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: amount at max, just above max, zero, negative.
- Include full `cargo test` output in the PR description.

### Example commit message
`refactor: enforce amount_validation bounds in create_contract and deposit`

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
title: "Add tests for the dispute resolution_payouts split math in dispute.rs"
labels: type:test, area:dispute, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for the dispute resolution_payouts split math in dispute.rs

### Description
[`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) implements `resolution_payouts` for `FullRefund`, `PartialRefund` (70/30), `FullPayout`, and `Split(client, freelancer)`, plus `final_status_after_resolution`. This pure money-splitting logic is the most security-sensitive code in the contract, yet it has no dedicated unit tests covering the 70/30 rounding, the `Split` total-must-equal-available rule, negative-split rejection, and the accounting-invariant guard.

This issue adds focused unit tests for the dispute payout calculations.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Test each `DisputeResolution` variant against a range of `available` balances, asserting `(client_payout, freelancer_payout)` and that the two always sum to `available`.
- Cover `PartialRefund` rounding at odd amounts (e.g. available = 7 → 30% truncation).
- Cover `Split` rejection when totals mismatch or amounts are negative (`InvalidDisputeSplit`).
- Cover the `AccountingInvariantViolated` path when `available` would be negative, and verify `final_status_after_resolution` returns `Refunded` only on full refund.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-dispute-payouts`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) — no logic change expected; only add a `#[cfg(test)]` module if needed.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/dispute.rs`](contracts/escrow/src/test/dispute.rs) — table-driven assertions over all variants.
  - **Add documentation:** describe the payout matrix in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`) where helpful.
  - Validate security assumptions: payouts conserve the available balance.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: zero available, odd-amount PartialRefund, mismatched Split.
- Include full `cargo test` output in the PR description.

### Example commit message
`test: add resolution_payouts split-math coverage for dispute.rs`

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
title: "Add property tests for resolution_payouts conserving the available balance"
labels: type:test, area:dispute, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add property tests for resolution_payouts conserving the available balance

### Description
The `Split` and `PartialRefund` arms of `resolution_payouts` in [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) must satisfy the invariant `client_payout + freelancer_payout == available` for all inputs that succeed. There is an existing `proptest.rs`/`fuzz_test.rs` harness in the crate but no property test pinning this conservation invariant for dispute payouts.

This issue adds a property test that fuzzes `available` and `Split` inputs to prove value is conserved and never created.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Generate random non-negative `available` values and `Split(client, freelancer)` candidates; assert success iff `client + freelancer == available` and both `>= 0`.
- Assert `FullRefund`, `FullPayout`, and `PartialRefund` always conserve `available` and never return negative payouts.
- Use the repo's existing proptest configuration and bound the input domain to avoid overflow.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-dispute-conservation-proptest`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) — expose helpers if needed for testing.
  - **Write comprehensive tests in:** [`contracts/escrow/src/proptest.rs`](contracts/escrow/src/proptest.rs) — conservation properties for dispute payouts.
  - **Add documentation:** note the invariant in [`docs/escrow/SECURITY.md`](docs/escrow/SECURITY.md).
  - Include NatSpec-style doc comments (`///`) where helpful.
  - Validate security assumptions: no value creation across any input.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: available = 0, max-bounded available, boundary splits.
- Include full `cargo test` output in the PR description.

### Example commit message
`test: add property tests for dispute payout conservation`

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
title: "Add tests asserting approval auto-expiry via temporary storage TTL"
labels: type:test, area:approvals, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests asserting approval auto-expiry via temporary storage TTL

### Description
`approve_milestone` stores approvals in temporary storage with `PENDING_APPROVAL_TTL_LEDGERS` (seven days) in [`contracts/escrow/src/approvals.rs`](contracts/escrow/src/approvals.rs), and `check_approvals` treats an expired/absent record as `InsufficientApprovals` (fail-closed). The inline unit tests cover approval and duplicate rejection but never advance the ledger past the TTL to prove that an aged approval actually stops a release.

This issue adds tests that advance the ledger sequence beyond the TTL and assert release fails.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Approve a milestone, advance `env.ledger()` sequence beyond `PENDING_APPROVAL_TTL_LEDGERS`, and assert `check_approvals` / `release_milestone` fails with `InsufficientApprovals`.
- Assert that a fresh approval within the bump threshold keeps the entry live.
- Cover each `ReleaseAuthorization` mode, including MultiSig where one approval expires before the second arrives.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-approval-ttl-expiry`
- Implement changes
  - **Write code in:** no production change expected.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/ttl_tests.rs`](contracts/escrow/src/test/ttl_tests.rs) and [`contracts/escrow/src/test/approval_expiry.rs`](contracts/escrow/src/test/approval_expiry.rs).
  - **Add documentation:** note expiry semantics in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`) where helpful.
  - Validate security assumptions: expired approvals cannot release funds.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: exactly-at-TTL, one ledger past TTL, refresh before expiry.
- Include full `cargo test` output in the PR description.

### Example commit message
`test: assert milestone approvals auto-expire via temporary TTL`

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
title: "Add tests for accept_client_migration with an expired pending migration"
labels: type:test, area:migration, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for accept_client_migration with an expired pending migration

### Description
`accept_client_migration` in [`contracts/escrow/src/migration.rs`](contracts/escrow/src/migration.rs) loads the pending record via `read_if_live`, which returns `None` once the `PENDING_MIGRATION_TTL_LEDGERS` (21-day) temporary entry has been evicted, and then panics with `InvalidState`. The existing client-migration tests cover propose/accept happy paths but do not advance the ledger past the migration TTL to confirm a stale proposal can no longer be accepted.

This issue adds tests covering expiry of the pending migration window.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Propose a migration, advance `env.ledger()` sequence beyond `PENDING_MIGRATION_TTL_LEDGERS`, and assert `accept_client_migration` panics with `InvalidState` and `has_pending_client_migration` returns false.
- Assert acceptance within the window still succeeds and updates `contract.client`.
- Cover acceptance by the wrong `new_client` (`UnauthorizedRole`).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-migration-expiry`
- Implement changes
  - **Write code in:** no production change expected.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/client_migration.rs`](contracts/escrow/src/test/client_migration.rs).
  - **Add documentation:** note the migration window in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`) where helpful.
  - Validate security assumptions: expired proposals cannot transfer client rights.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: accept at boundary, accept after expiry, accept by wrong address.
- Include full `cargo test` output in the PR description.

### Example commit message
`test: cover expired pending client migration acceptance`

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
title: "Add finalize_contract authorization and status-gate negative-path tests"
labels: type:test, area:finalization, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add finalize_contract authorization and status-gate negative-path tests

### Description
`finalize_contract` in [`contracts/escrow/src/finalize.rs`](contracts/escrow/src/finalize.rs) enforces several preconditions: not paused, valid participant via `require_finalizer_role`, status must be `Completed` or `Disputed`, and no existing finalization record. Each of these is a distinct rejection path (`ContractPaused`, `UnauthorizedRole`, `InvalidStatusTransition`, `AlreadyFinalized`), but there is no dedicated test module asserting every guard, nor that a finalized contract blocks later `release_milestone` / `refund_unreleased_milestones` calls.

This issue adds comprehensive negative-path coverage for finalization.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Assert finalize fails while paused/emergency, by a non-participant, from a non-`Completed`/`Disputed` status, and when already finalized.
- Assert that after finalization, `release_milestone` and `refund_unreleased_milestones` panic with `AlreadyFinalized` (via `require_not_finalized`).
- Assert the emitted `finalized` event and that `get_finalization_record` returns the snapshot.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-finalize-negative-paths`
- Implement changes
  - **Write code in:** no production change expected.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/security.rs`](contracts/escrow/src/test/security.rs) — finalize guard coverage.
  - **Add documentation:** confirm the guard list in [`docs/escrow/SECURITY.md`](docs/escrow/SECURITY.md).
  - Include NatSpec-style doc comments (`///`) where helpful.
  - Validate security assumptions: every finalize guard is enforced.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: each rejection path plus a successful finalize then blocked mutation.
- Include full `cargo test` output in the PR description.

### Example commit message
`test: add finalize_contract authorization and status-gate coverage`

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
title: "Add release_milestone tests for each ReleaseAuthorization mode's authorized callers"
labels: type:test, area:release, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add release_milestone tests for each ReleaseAuthorization mode's authorized callers

### Description
`release_milestone` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) branches on `ReleaseAuthorization::{ClientOnly, ArbiterOnly, ClientAndArbiter, MultiSig}` to decide which caller may release, then separately calls `approvals::check_approvals` which requires a matching approval. The interaction between the caller-authorization match and the approval check is subtle (e.g. MultiSig permits client or freelancer to *call* but requires both to have *approved*), and there is no test matrix exercising every mode's authorized and unauthorized callers end-to-end.

This issue adds a full caller-by-mode authorization matrix for releases.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- For each `ReleaseAuthorization` mode, assert which callers succeed and which panic with `UnauthorizedRole`.
- For MultiSig, assert release requires both client and freelancer approvals and that a single approval yields `InsufficientApprovals`.
- Assert releasing without any approval fails, and that releasing in non-`Funded` status fails with `InvalidState`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-release-auth-matrix`
- Implement changes
  - **Write code in:** no production change expected.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/release_authorization.rs`](contracts/escrow/src/test/release_authorization.rs).
  - **Add documentation:** tabulate the mode/caller matrix in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`) where helpful.
  - Validate security assumptions: only authorized callers with valid approvals release.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: arbiter-only with client caller, MultiSig with one approval, no-approval release.
- Include full `cargo test` output in the PR description.

### Example commit message
`test: add ReleaseAuthorization caller/approval matrix for release_milestone`

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
title: "Add deposit_funds tests for non-client callers and over-funding behavior"
labels: type:test, area:deposits, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add deposit_funds tests for non-client callers and over-funding behavior

### Description
`deposit_funds` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) rejects non-client callers with `UnauthorizedRole`, rejects non-positive amounts with `AmountMustBePositive`, and only transitions to `Funded` once `funded_amount >= total_amount` — but it does **not** cap deposits at the milestone total, so a client can over-fund. The README asserts "deposits cannot exceed the required escrow total," which the code does not currently enforce. The behavior here needs locked-in tests so the discrepancy is visible and any cap change is covered.

This issue adds deposit-path tests for callers and amounts, documenting actual vs intended over-funding behavior.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Assert a non-client caller panics with `UnauthorizedRole` and a zero/negative amount panics with `AmountMustBePositive`.
- Assert the `Created → Funded` transition at exactly the total and document whether over-funding is accepted (current behavior) or rejected.
- If the intended behavior is a cap, add a failing test and an accompanying fix; otherwise pin current behavior with a comment referencing the README discrepancy.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-deposit-caller-and-overfund`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) only if a cap is added.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/deposit.rs`](contracts/escrow/src/test/deposit.rs).
  - **Add documentation:** reconcile the over-funding claim in [`README.md`](README.md).
  - Include NatSpec-style doc comments (`///`) where helpful.
  - Validate security assumptions: only the client funds; transitions are correct.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: freelancer caller, arbiter caller, exact-total, over-total deposit.
- Include full `cargo test` output in the PR description.

### Example commit message
`test: add deposit_funds caller and over-funding coverage`

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
title: "Add get_refundable_balance accounting tests across mixed release and refund states"
labels: type:test, area:accounting, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add get_refundable_balance accounting tests across mixed release and refund states

### Description
`get_refundable_balance` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) returns `funded_amount - released_amount - refunded_amount`, the core accounting invariant the whole contract relies on. Despite being security-critical, there is no test that drives a contract through a mix of releases and refunds and asserts this value stays correct (and non-negative) at every step.

This issue adds accounting tests that verify the refundable balance after each release/refund operation.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Create a multi-milestone contract, fund it, then interleave releases and refunds, asserting `get_refundable_balance` equals `funded - released - refunded` after each step.
- Assert the value never goes negative and reaches zero only when all milestones are terminal.
- Cross-check against `get_contract` fields and the `ContractStatus` transitions.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-refundable-balance`
- Implement changes
  - **Write code in:** no production change expected.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/refund.rs`](contracts/escrow/src/test/refund.rs).
  - **Add documentation:** state the accounting invariant in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`) where helpful.
  - Validate security assumptions: invariant holds across every operation order.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: release-then-refund, refund-then-release, all-released, all-refunded.
- Include full `cargo test` output in the PR description.

### Example commit message
`test: verify get_refundable_balance across mixed release/refund states`

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
title: "Add create_contract arbiter-requirement tests for each ReleaseAuthorization mode"
labels: type:test, area:create-contract, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add create_contract arbiter-requirement tests for each ReleaseAuthorization mode

### Description
`create_contract` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) requires an arbiter for `ArbiterOnly` and `ClientAndArbiter` modes (panicking `MissingArbiter` if absent) and rejects an arbiter equal to the client or freelancer (`InvalidArbiter`). It also rejects `client == freelancer` (`InvalidParticipant`). These branch conditions are exactly the kind of input validation that needs exhaustive coverage, but there is no dedicated test asserting each mode's arbiter requirement and the invalid-arbiter rejections.

This issue adds creation-time validation tests for the arbiter/participant rules.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- For each `ReleaseAuthorization` mode, assert whether `arbiter: None` is accepted or rejected with `MissingArbiter`.
- Assert `InvalidArbiter` when the arbiter equals the client or the freelancer.
- Assert `InvalidParticipant` when client equals freelancer, and `EmptyMilestones` / `InvalidMilestoneAmount` for bad milestone vectors.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-create-arbiter-rules`
- Implement changes
  - **Write code in:** no production change expected.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/create_contract_bounds.rs`](contracts/escrow/src/test/create_contract_bounds.rs).
  - **Add documentation:** tabulate arbiter requirements in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`) where helpful.
  - Validate security assumptions: invalid participant configs never persist.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: ArbiterOnly without arbiter, arbiter == client, client == freelancer.
- Include full `cargo test` output in the PR description.

### Example commit message
`test: cover create_contract arbiter and participant validation`

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
title: "Add emergency-pause vs normal-pause distinction tests for unpause and resolve_emergency"
labels: type:test, area:pause, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add emergency-pause vs normal-pause distinction tests for unpause and resolve_emergency

### Description
`unpause` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) refuses to run while the `Emergency` flag is set (panicking `EmergencyActive`), so an operator must call `resolve_emergency` first. `activate_emergency_pause` sets both `Emergency` and `Paused`, while `pause` sets only `Paused`. This two-tier model is easy to get wrong, and there is no test proving that `unpause` cannot clear an emergency or that `resolve_emergency` clears both flags.

This issue adds tests pinning the interaction between the two pause tiers.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Assert `pause` then `unpause` toggles `Paused` while `Emergency` stays false.
- Assert `activate_emergency_pause` sets both flags and that `unpause` then panics with `EmergencyActive`.
- Assert `resolve_emergency` clears both `Emergency` and `Paused`, and that all controls require admin auth.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-pause-tier-distinction`
- Implement changes
  - **Write code in:** no production change expected.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/pause_controls.rs`](contracts/escrow/src/test/pause_controls.rs).
  - **Add documentation:** clarify the two-tier model in [`docs/escrow/emergency-controls.md`](docs/escrow/emergency-controls.md).
  - Include NatSpec-style doc comments (`///`) where helpful.
  - Validate security assumptions: emergency cannot be cleared by a plain unpause.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: unpause during emergency, double resolve, non-admin caller.
- Include full `cargo test` output in the PR description.

### Example commit message
`test: distinguish normal pause from emergency pause in controls`

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
title: "Add get_contract and get_milestones not-found tests for unknown contract ids"
labels: type:test, area:reads, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add get_contract and get_milestones not-found tests for unknown contract ids

### Description
`get_contract`, `get_milestones`, and `get_refundable_balance` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) all panic with `ContractNotFound` for unknown ids, and each also bumps TTL on a successful read. There is a `test_read_notfound.rs` stub in the crate, but the read-path not-found behavior for these public getters is not systematically asserted, and the TTL-on-read side effect is untested.

This issue adds not-found and read-side-effect tests for the public getters.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Assert `get_contract`, `get_milestones`, and `get_refundable_balance` panic with `ContractNotFound` for an id that was never created.
- Assert a successful read returns the expected data and that `get_milestone_approvals` returns `None` when no approval exists.
- Where feasible, assert the read extends persistent TTL (e.g. via storage introspection helpers).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-read-notfound`
- Implement changes
  - **Write code in:** no production change expected.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/persistence.rs`](contracts/escrow/src/test/persistence.rs).
  - **Add documentation:** note getter semantics in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`) where helpful.
  - Validate security assumptions: reads never mutate balances.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: unknown id, id zero, valid id after creation.
- Include full `cargo test` output in the PR description.

### Example commit message
`test: cover not-found behavior for public escrow getters`

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
title: "Reconcile the README pause matrix with the actually enforced pause guards"
labels: type:docs, area:documentation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Reconcile the README pause matrix with the actually enforced pause guards

### Description
[`README.md`](README.md) states that "when paused, all mutating escrow operations (`create_contract`, `deposit_funds`, `release_milestone`, `issue_reputation`, `cancel_contract`) are blocked with `ContractPaused`," but in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) only `finalize_contract` and the migration entrypoints call `require_not_paused`. The README also references `contracts/escrow/src/test/performance.rs` and `flows.rs`, whose presence does not match the inline test modules. This drift misleads integrators about which operations are actually halted while paused.

This issue corrects the documentation to describe the real, enforced behavior (or coordinates with the enforcement fix).

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Audit which entrypoints actually call `require_not_paused` and update the README pause matrix to match exactly.
- Fix README references to test files that do not exist, pointing to the real `contracts/escrow/src/test/` modules.
- If a companion enforcement PR lands the missing guards, update this doc to reflect the post-fix state and cross-link it.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-pause-matrix-reconcile`
- Implement changes
  - **Write code in:** no production change; documentation only in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) doc comments if clarifying.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/pause_controls.rs`](contracts/escrow/src/test/pause_controls.rs) — assertions matching the documented matrix.
  - **Add documentation:** update [`README.md`](README.md) and [`docs/escrow/emergency-controls.md`](docs/escrow/emergency-controls.md).
  - Include NatSpec-style doc comments (`///`) on affected entrypoints.
  - Validate security assumptions: docs match enforcement.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: verify each documented entrypoint's pause behavior in tests.
- Include full `cargo test` output in the PR description.

### Example commit message
`docs: reconcile README pause matrix with enforced guards`

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
title: "Document the dispute resolution model and DisputeResolution variants"
labels: type:docs, area:documentation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the dispute resolution model and DisputeResolution variants

### Description
[`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) defines the economic core of dispute handling — `DisputeResolution::{FullRefund, PartialRefund, FullPayout, Split}`, the fixed 70/30 split, `resolution_payouts`, and `final_status_after_resolution` — but there is no reviewer-facing document explaining when an arbiter would choose each option, how the split is computed and rounded, or how the resulting status is derived. Integrators and auditors must read the Rust to understand the payout policy.

This issue adds a dedicated dispute-model doc.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Document each `DisputeResolution` variant, including the exact 70/30 `PartialRefund` formula and its truncation/rounding behavior.
- Explain the `Split` invariant (`client + freelancer == available`, both non-negative) and the `InvalidDisputeSplit` / `AccountingInvariantViolated` errors.
- Describe how `final_status_after_resolution` chooses `Refunded` vs `Completed`, with a worked numeric example.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-dispute-model`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) — enrich `///` doc comments only.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/dispute.rs`](contracts/escrow/src/test/dispute.rs) — doctest-style assertions matching the documented examples.
  - **Add documentation:** add `docs/escrow/dispute-resolution.md`.
  - Include NatSpec-style doc comments (`///`) on the public items.
  - Validate security assumptions: documented math matches implemented math.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: ensure documented examples are backed by tests.
- Include full `cargo test` output in the PR description.

### Example commit message
`docs: document the dispute resolution model and payout variants`

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
title: "Document the TTL and storage-expiry policy for transient and persistent entries"
labels: type:docs, area:documentation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the TTL and storage-expiry policy for transient and persistent entries

### Description
[`contracts/escrow/src/ttl.rs`](contracts/escrow/src/ttl.rs) encodes the contract's whole storage-lifetime policy in constants — `LEDGERS_PER_DAY`, `PENDING_APPROVAL_TTL_LEDGERS` (7 days), `PENDING_MIGRATION_TTL_LEDGERS` (21 days), `PERSISTENT_TTL_LEDGERS` (30 days), and their bump thresholds — plus helpers like `read_if_live` whose "expired vs never-set both return None" semantics are subtle. None of this is captured in reviewer docs, so integrators cannot reason about when approvals/migrations expire or when a contract entry could be evicted.

This issue documents the TTL model and its implications.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Tabulate every TTL constant with its ledger value, day equivalent, and which keys it governs.
- Explain the bump-on-read strategy in `extend_contract_ttl` / `extend_milestone_ttl` and the eviction risk when a contract is untouched past `PERSISTENT_TTL_LEDGERS`.
- Clarify the `read_if_live` "None means expired or absent" behavior and its security implication (fail-closed approvals/migrations).

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-ttl-policy`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/ttl.rs`](contracts/escrow/src/ttl.rs) — enrich module-level `//!` docs only.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/ttl_tests.rs`](contracts/escrow/src/test/ttl_tests.rs) — assertions matching the documented day-equivalents.
  - **Add documentation:** expand [`docs/escrow/state-persistence.md`](docs/escrow/state-persistence.md).
  - Include NatSpec-style doc comments (`///`) on the TTL helpers.
  - Validate security assumptions: documented expiry matches constants.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: ensure documented TTL values are asserted in tests.
- Include full `cargo test` output in the PR description.

### Example commit message
`docs: document escrow TTL and storage-expiry policy`

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
title: "Document the ReadinessChecklist fields and the mainnet readiness workflow"
labels: type:docs, area:documentation, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the ReadinessChecklist fields and the mainnet readiness workflow

### Description
`ReadinessChecklist` in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) tracks `initialized`, `governed_params_set`, and `emergency_controls_enabled`, surfaced via `get_mainnet_readiness_info` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs). The flags are flipped in scattered places (`initialize`, governed-parameter setters, `activate_emergency_pause`), but there is no document explaining what "mainnet ready" means, which flags must be true before deployment, or how an operator should verify them.

This issue documents the readiness checklist as a deploy-gating runbook.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Describe each `ReadinessChecklist` field, where it is set, and why it gates production readiness.
- Provide an operator checklist: initialize → set governed params → exercise emergency controls → verify via `get_mainnet_readiness_info`.
- Note that `emergency_controls_enabled` is set by *activating* an emergency pause and discuss the implication for a clean deploy.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-readiness-checklist`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — enrich `///` docs on `get_mainnet_readiness_info` only.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/mainnet_readiness.rs`](contracts/escrow/src/test/mainnet_readiness.rs) — assertions matching the documented flag transitions.
  - **Add documentation:** add `docs/escrow/mainnet-readiness.md`.
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: readiness flags reflect real state.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: each flag false then true after the corresponding action.
- Include full `cargo test` output in the PR description.

### Example commit message
`docs: document ReadinessChecklist and mainnet readiness workflow`

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
title: "Document the ContractSummary indexer schema and its versioning policy"
labels: type:docs, area:documentation, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the ContractSummary indexer schema and its versioning policy

### Description
`ContractSummary`, `MilestoneSummary`, and `CONTRACT_SUMMARY_SCHEMA_VERSION` in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) define the off-chain indexer contract, consumed by the finalization snapshot in [`contracts/escrow/src/finalize.rs`](contracts/escrow/src/finalize.rs). The schema includes derived fields like `refundable_balance` and `released_milestone_count`, but there is no document explaining each field's meaning, how it is computed, or what bumping `schema_version` (currently `1`) implies for downstream indexers.

This issue documents the summary schema and a versioning policy for it.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Describe every `ContractSummary` and `MilestoneSummary` field and how `summarize_contract` derives it (including the `safe_subtract_amounts` refundable-balance computation).
- Define a versioning policy: when to bump `CONTRACT_SUMMARY_SCHEMA_VERSION`, backward-compatibility expectations, and how indexers should branch on it.
- Note the current `reputation_issued` hardcoding caveat and cross-link any fix.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-summary-schema`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) — enrich `///` docs on the summary types only.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/persistence.rs`](contracts/escrow/src/test/persistence.rs) — assert summary fields match documented derivations.
  - **Add documentation:** add `docs/escrow/indexer-schema.md`.
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: documented fields match computed values.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: summary for partially-released and fully-refunded contracts.
- Include full `cargo test` output in the PR description.

### Example commit message
`docs: document ContractSummary indexer schema and versioning`

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
title: "Add Soroban testnet deployment and initialization runbook for the escrow contract"
labels: type:docs, area:deployment, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add Soroban testnet deployment and initialization runbook for the escrow contract

### Description
The [`README.md`](README.md) covers building and testing but only mentions the Stellar CLI as "optional ... for deployment workflows" — there is no end-to-end runbook for compiling the WASM, deploying the escrow contract to Soroban testnet, and calling `initialize(admin)` to bind the admin before use. Given that `initialize` is single-use and that uninitialized contracts lack pause protection, an undocumented deploy is risky.

This issue adds a concrete deployment-and-initialization runbook.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Document building the optimized WASM (`cargo build --target wasm32-unknown-unknown --release` / `stellar contract build`) and deploying via the Stellar CLI to testnet.
- Show the exact `initialize`, `set_protocol_fee_bps`, and governance-setup invocations an operator must run post-deploy, referencing real entrypoints in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs).
- Include a verification step using `get_admin` and `get_mainnet_readiness_info`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-deployment-runbook`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — only `///` doc clarifications on `initialize` if needed.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/mainnet_readiness.rs`](contracts/escrow/src/test/mainnet_readiness.rs) — assert the documented init sequence leaves the contract ready.
  - **Add documentation:** add `docs/escrow/deployment.md` and link it from [`README.md`](README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: documented sequence binds the admin before any value flow.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: re-initialization rejection, readiness verification after init.
- Include full `cargo test` output in the PR description.

### Example commit message
`docs: add Soroban testnet deployment and initialization runbook`

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
title: "Add an average_rating accessor and store rating_count in the Reputation record"
labels: type:enhancement, area:reputation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add an average_rating accessor and store rating_count in the Reputation record

### Description
The `Reputation` struct in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) stores `completed_contracts`, `total_rating`, and `last_rating`, and `issue_reputation` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) accumulates `total_rating += rating` and `completed_contracts += 1`. But there is no way to read a freelancer's average rating: a consumer must fetch the record and divide `total_rating / completed_contracts` itself, and `completed_contracts` doubles as the rating count only by coincidence. There is no overflow-safe accessor exposed on-chain.

This issue adds a first-class average-rating reader with explicit, safe division.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `get_average_rating(addr) -> Option<i128>` (scaled, e.g. basis points, to preserve fractional precision) computed as `total_rating * SCALE / completed_contracts` with checked arithmetic and a zero-contracts guard returning `None`.
- Keep the existing `Reputation` fields and the once-per-contract issuance guard intact.
- Document the scaling factor so clients interpret the value correctly.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-average-rating-accessor`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — new `get_average_rating` reader.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/reputation.rs`](contracts/escrow/src/test/reputation.rs) — assert averages across multiple ratings and the zero-contracts case.
  - **Add documentation:** describe the scaled average in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: no divide-by-zero, no overflow.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: single rating, multiple ratings, no ratings.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: add scaled average_rating accessor for reputation records`

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
title: "Emit an initialized event payload that also records the protocol fee and parameters"
labels: type:enhancement, area:events, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Emit an initialized event payload that also records the protocol fee and parameters

### Description
`initialize` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) emits an `(init, admin_set)` event carrying `(admin, timestamp)` but nothing about the initial protocol configuration, because `ProtocolFeeBps` and `GovernedParameters` are set separately afterward. Indexers therefore cannot capture the contract's starting fee/parameter state from a single bootstrap event, and there is no event at all when `set_protocol_fee_bps` or governed params are configured during bring-up beyond the governance module's own events.

This issue enriches the initialization event so the bootstrap configuration is observable.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Extend the `init` event payload (or add a follow-up `config` event) to include the initial `protocol_fee_bps` and any default governed parameters.
- Keep the event topics consistent with existing conventions and do not change `initialize`'s single-use semantics.
- Ensure the event is emitted only after the readiness checklist's `initialized` flag is set.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-init-config-event`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — enrich the init event publish.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/mainnet_readiness.rs`](contracts/escrow/src/test/mainnet_readiness.rs) — assert the event payload contents.
  - **Add documentation:** list the init event schema in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: event reflects real persisted config.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: default fee zero, nonzero initial fee.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: enrich initialize event with protocol fee and parameters`

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
title: "Add a get_protocol_fee_bps public reader and accumulated-fees view"
labels: type:enhancement, area:protocol-fees, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a get_protocol_fee_bps public reader and accumulated-fees view

### Description
`get_protocol_fee_bps` and `is_initialized` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) are private (`fn`, not `pub fn`), and `DataKey::AccumulatedProtocolFees` is written during `release_milestone` but never exposed through a reader. Off-chain consumers and integrators cannot query the current fee rate or how much protocol fee has accrued without replaying every release event, which is awkward and error-prone.

This issue exposes public read-only views for the fee rate and accumulated fees.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `pub fn get_protocol_fee_bps(env) -> u32` and `pub fn get_accumulated_protocol_fees(env) -> i128`, both defaulting to `0` when unset.
- Reuse the existing private fee helpers where possible without breaking the internal `release_milestone` fee path.
- Keep the readers non-mutating (no TTL bump required) and document their semantics.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-fee-readers`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — public fee/accumulated-fee readers.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/protocol_fees.rs`](contracts/escrow/src/test/protocol_fees.rs) — assert readers reflect set rate and accrued fees after releases.
  - **Add documentation:** document the readers in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: readers never mutate state.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: zero fee, post-release accrual, multiple releases.
- Include full `cargo test` output in the PR description.

### Example commit message
`feat: expose public protocol fee rate and accumulated fee readers`

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
title: "Replace per-module Symbol::new(\"milestones\") with a shared symbol_short constant"
labels: type:enhancement, area:performance, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Replace per-module Symbol::new("milestones") with a shared symbol_short constant

### Description
Across [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs), [`contracts/escrow/src/finalize.rs`](contracts/escrow/src/finalize.rs), and [`contracts/escrow/src/approvals.rs`](contracts/escrow/src/approvals.rs), the milestones key is rebuilt every call with `Symbol::new(&env, "milestones")`, which constructs the symbol from a string at runtime on each invocation. The string `"milestones"` is 10 characters, within the 9-character `symbol_short!` limit only if shortened — but the repeated runtime construction adds avoidable host-call overhead to hot paths like `release_milestone` and `deposit_funds`.

This issue centralizes the milestone symbol to a single definition and reduces per-call construction cost.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Define the milestone key symbol in one place (a shared helper returning the symbol, or a `symbol_short!`-backed key if the name is shortened) and use it everywhere.
- Verify the storage key bytes are unchanged so existing persisted entries remain readable; if shortening the name, provide a documented migration note.
- Confirm the change is purely an internal optimization with identical externally observable behavior.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-milestone-symbol-const`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — shared milestone-symbol helper used across modules.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/persistence.rs`](contracts/escrow/src/test/persistence.rs) — assert keys still resolve to the same stored vectors.
  - **Add documentation:** note the key derivation in [`docs/escrow/state-persistence.md`](docs/escrow/state-persistence.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: storage keys remain stable.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: read existing milestone vectors after the change, write/read round-trip.
- Include full `cargo test` output in the PR description.

### Example commit message
`perf: centralize the milestones storage symbol to cut per-call overhead`

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
title: "Validate Split dispute amounts and arbiter authorization before applying payouts"
labels: type:enhancement, area:dispute, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Validate Split dispute amounts and arbiter authorization before applying payouts

### Description
`resolution_payouts` in [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) validates that a `Split(client, freelancer)` is non-negative and sums to `available`, but it is a pure function with no notion of *who* may choose a resolution — there is no entrypoint that ties resolution selection to the assigned arbiter's authorization, and the module is not yet wired into a guarded `resolve_dispute` flow with status checks. The split math is correct, but its caller guarantees are unspecified.

This issue adds the authorization and status preconditions around applying a dispute resolution.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Require the assigned `arbiter` to `require_auth()` before any resolution is applied, rejecting non-arbiters with `UnauthorizedRole`.
- Require the contract to be in `Disputed` status before resolution and reject otherwise with `InvalidState`.
- Reuse `resolution_payouts` for the math and `final_status_after_resolution` for the resulting status, persisting `refunded_amount`/`released_amount` consistently with the existing accounting rules.
- Emit a `dispute_resolved` event carrying the `DisputeResolution::code()` and the computed payouts.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-dispute-auth-guards`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) and [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — arbiter-gated resolution application.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/dispute.rs`](contracts/escrow/src/test/dispute.rs) — assert non-arbiter rejection and non-Disputed status rejection.
  - **Add documentation:** describe the guarded flow in [`docs/escrow/README.md`](docs/escrow/README.md).
  - Include NatSpec-style doc comments (`///`).
  - Validate security assumptions: only the arbiter resolves, only while Disputed.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases: client attempts resolution, resolution on non-disputed contract, valid Split.
- Include full `cargo test` output and a security notes section in the PR description.

### Example commit message
`feat: gate dispute resolution behind arbiter auth and Disputed status`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
