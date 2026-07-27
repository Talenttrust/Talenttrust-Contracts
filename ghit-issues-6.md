---
type: Feature
title: "Transfer escrowed funds on dispute resolution in resolve_dispute"
labels: type:feature, area:dispute, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Transfer escrowed funds on dispute resolution in resolve_dispute

### Description
`resolve_dispute` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) computes `(client_payout, freelancer_payout)` via `dispute::resolution_payouts`, mutates `contract.refunded_amount` and `contract.released_amount`, sets the final status, and emits an event — but it **never moves any tokens**. Every other money-flow path (`release_milestone`, `refund_unreleased_milestones`, `cancel_contract`) performs a `token::Client::transfer` from the contract's escrow balance; `resolve_dispute` is the only one that updates accounting while leaving the SAC token balance untouched. The result is that resolved disputes credit nobody on-chain: the freelancer's awarded payout and the client's refund both remain stranded in the contract address, and the accounting fields no longer match the real token balance.

This issue makes dispute resolution actually pay out: after computing the split, transfer `client_payout` to `contract.client` and `freelancer_payout` to `contract.freelancer` from the bound settlement token, atomically with the accounting update.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Read the bound settlement token via `Self::read_settlement_token(&env)` and resolve to a typed error (`SettlementTokenNotConfigured`) rather than a host `expect` panic if unset.
- After `resolution_payouts` returns, transfer `client_payout` to `contract.client` and `freelancer_payout` to `contract.freelancer`, skipping zero-value transfers.
- Keep the existing accounting invariant check (`released + refunded == funded`) and only mutate state after the transfers are staged so a transfer failure cannot leave inconsistent accounting.
- Preserve the arbiter `require_auth`, the `Disputed`-state gate, the pause/emergency gate, and the `dispute resolved` event (extend the payload with the actual transferred amounts).
- Apply this fix to the canonical `resolve_dispute` only; do not resurrect the duplicate definitions in `dispute.rs`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-dispute-payout-transfer`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the two `soroban_sdk::token` transfers into `resolve_dispute` after the payout computation.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/dispute.rs`](contracts/escrow/src/test/dispute.rs) — register a mock SAC, fund a disputed contract, and assert post-resolution token balance deltas for client and freelancer across FullRefund, PartialRefund, FullPayout, and Split.
  - **Add documentation:** update `docs/escrow/` dispute notes to state that resolution settles on-chain.
  - Include NatSpec-style doc comments (`///`) on the changed entrypoint matching the existing style in `lib.rs`.
  - Validate security assumptions: no double-pay across repeated resolution, no overdraw beyond available balance, zero-transfer skipping, correct arbiter auth.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: zero available balance, unconfigured settlement token, non-arbiter caller, and a non-Disputed contract.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`feat: transfer escrowed funds to client and freelancer on dispute resolution with tests`

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
title: "Reconcile the deposit-to-release state gap that leaves Funded contracts unreleasable"
labels: type:enhancement, area:state-machine, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Reconcile the deposit-to-release state gap that leaves Funded contracts unreleasable

### Description
`deposit::deposit_funds_impl` in [`contracts/escrow/src/deposit.rs`](contracts/escrow/src/deposit.rs) promotes a contract to `ContractStatus::Funded` once `funded_amount >= total_amount`. But `release_milestone` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) guards with `if contract.status != ContractStatus::Accepted { panic InvalidState }`. There is no entrypoint anywhere in the contract that transitions a contract from `Funded` to `Accepted`. The consequence is a hard dead-end: a fully funded contract can never have any milestone released, because the only status `release_milestone` accepts is unreachable through the normal lifecycle.

This issue reconciles the two ends of the state machine so that a funded contract is releasable. Pick one canonical contract: either have `release_milestone` accept `Funded` (and `Accepted`) contracts, or wire a real `Funded → Accepted` transition into the lifecycle and document it as the required pre-release step.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Audit which statuses each money-flow entrypoint expects (`deposit_funds`, `release_milestone`, `raise_dispute`, `refund_unreleased_milestones`) and converge on a single documented progression.
- If `release_milestone` is changed to accept `Funded`, keep all other gates (approvals, per-milestone funding, pause/emergency, finalization) intact.
- Ensure the chosen path does not silently allow releases on `Created`, `Disputed`, `Cancelled`, `Refunded`, or `Completed` contracts.
- Update the `ContractStatus` doc comments and any README/docs state-machine description to match the reconciled flow.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-deposit-release-state-gap`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — adjust the `release_milestone` status gate (and/or add the missing transition) so funded contracts are releasable.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/release.rs`](contracts/escrow/src/test/release.rs) — assert a deposit-then-release happy path end to end, and that disallowed statuses still reject.
  - **Add documentation:** update the `ContractStatus` state-machine docs to show the canonical funded-to-release path.
  - Include NatSpec-style doc comments (`///`) describing the accepted statuses on `release_milestone`.
  - Validate security assumptions: no release from a never-funded or terminal contract, no bypass of approval checks.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: release attempt on `Created`, `Disputed`, `Cancelled`, and `Completed` contracts.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`fix: reconcile deposit-to-release status gate so funded contracts are releasable`

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
title: "Validate deposit_funds inputs before moving tokens to prevent transfer-then-revert"
labels: type:security, area:deposit, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Validate deposit_funds inputs before moving tokens to prevent transfer-then-revert

### Description
The public `deposit_funds` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) calls `token_client.transfer(&caller, &env.current_contract_address(), &amount)` **first**, and only afterward delegates to `deposit::deposit_funds_impl`, which is where `amount > 0`, `caller == client`, `ContractNotFound`, and `status == Created` are validated. This ordering is wrong: a negative `amount` will reach the token transfer before `AmountMustBePositive` is checked, an unknown `contract_id` triggers a host token call before `ContractNotFound`, and a non-client caller's `require_auth`/role check happens only after funds have already been pulled. The settlement token is also read with `.expect("Settlement token not set")`, a host panic rather than a typed error.

This issue moves all validation ahead of the transfer so the contract never pulls tokens for a call that will revert, and replaces the `expect` with a typed `SettlementTokenNotConfigured` error.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Load the contract, verify it exists, verify `caller == contract.client`, run `caller.require_auth()`, verify `status == Created`, and verify `amount > 0` — all before any `token::Client::transfer`.
- Replace `read_settlement_token(...).expect(...)` with a typed error path returning `SettlementTokenNotConfigured`.
- Keep `deposit_funds_impl` as the single source of accounting truth; refactor so validation is not duplicated between the wrapper and the impl.
- Preserve the `Funded` promotion behavior and any deposit event semantics.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-deposit-validate-before-transfer`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — reorder validation before the transfer; thread checks through `deposit::deposit_funds_impl` in [`contracts/escrow/src/deposit.rs`](contracts/escrow/src/deposit.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/deposit.rs`](contracts/escrow/src/test/deposit.rs) — assert that negative-amount, wrong-caller, and unknown-contract deposits revert with no token movement (balance unchanged).
  - **Add documentation:** note the validate-before-transfer ordering in the `deposit_funds` doc comment.
  - Include NatSpec-style doc comments (`///`) on the changed entrypoint.
  - Validate security assumptions: no token pull on any reverting path, unconfigured token yields a typed error.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: zero/negative amount, non-client caller, unknown contract id, unset settlement token.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`fix: validate deposit_funds inputs before token transfer and use a typed token-unset error`

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
title: "Make bind_settlement_token reject rebinding once a token is already set"
labels: type:security, area:settlement-token, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Make bind_settlement_token reject rebinding once a token is already set

### Description
`bind_settlement_token` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) documents a `SettlementTokenAlreadyBound` error in its `# Errors` section, but the implementation never checks for an existing binding — `write_settlement_token` unconditionally overwrites `DataKey::SettlementToken`. Because every fund movement (deposit, release, refund, cancel, fee withdrawal) reads this single token, an admin (or a compromised admin key) can swap the settlement token mid-lifecycle to a different asset while the contract still holds the original token's balance. That silently strands the original deposits and breaks the one-asset-per-escrow invariant the rest of the code assumes.

This issue makes the binding write-once: `bind_settlement_token` (and its `set_settlement_token` alias) must fail with a typed error if a token is already bound, matching the documented contract.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add the `SettlementTokenAlreadyBound` variant to the canonical `Error` enum (append-only to preserve client-SDK stability).
- In `bind_settlement_token`, panic with that error when `read_settlement_token(&env).is_some()`, before writing.
- Ensure the `set_settlement_token` alias inherits the same guard (it already delegates).
- Keep the existing `NotInitialized` and `UnauthorizedRole` gates and the admin `require_auth`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-settlement-token-write-once`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the already-bound guard in `bind_settlement_token`; add the error variant in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/sac_custody.rs`](contracts/escrow/src/test/sac_custody.rs) — assert the first bind succeeds, a second bind reverts with `SettlementTokenAlreadyBound`, and the original token remains bound.
  - **Add documentation:** confirm the write-once semantics in the `bind_settlement_token` doc comment.
  - Include NatSpec-style doc comments (`///`) reflecting the new error path.
  - Validate security assumptions: no mid-lifecycle asset swap, no rebind even by the admin.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: rebind by admin, rebind by non-admin, rebind before initialization.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`fix: reject settlement-token rebinding to enforce write-once binding`

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
title: "Add a set_milestone_deadline entrypoint to populate the unused Milestone.deadline field"
labels: type:feature, area:milestones, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a set_milestone_deadline entrypoint to populate the unused Milestone.deadline field

### Description
The `Milestone` struct in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs) carries a `deadline: Option<u64>` field, and `is_milestone_overdue` plus the timeout-refund branch in `refund_unreleased_milestones` already read it. But `create_contract` always sets `deadline: None`, and **no entrypoint ever assigns a deadline**. The overdue logic is therefore dead in practice: every milestone is created without a deadline, so `is_milestone_overdue` always returns `false` and the timeout-refund path can never trigger.

This issue adds a `set_milestone_deadline(contract_id, caller, milestone_index, deadline)` entrypoint so the client can attach (or clear) a deadline on an unreleased, unrefunded milestone, making the existing overdue and timeout-refund machinery actually usable.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Require `caller.require_auth()` and that `caller == contract.client`; reject other roles with `UnauthorizedRole`.
- Allow setting a deadline only while the contract is in an active state and the target milestone is neither released nor refunded.
- Validate that `deadline` is strictly in the future relative to `now_seconds(&env)`; reject past timestamps.
- Gate behind pause/emergency and finalization checks, consistent with the other mutating entrypoints.
- Emit a `deadline_set` event with `(milestone_index, deadline, timestamp)` and bump milestone TTL.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-set-milestone-deadline`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add `set_milestone_deadline`, loading milestones via `ttl::load_milestones`/`store_milestones`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/timeout_tests.rs`](contracts/escrow/src/test/timeout_tests.rs) — set a deadline, advance ledger time, and assert `is_milestone_overdue` flips and a timeout refund succeeds.
  - **Add documentation:** document the deadline lifecycle in `docs/escrow/`.
  - Include NatSpec-style doc comments (`///`) on the new entrypoint.
  - Validate security assumptions: only the client sets deadlines, no deadline on released/refunded milestones, no past-dated deadlines.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: non-client caller, past timestamp, released milestone, out-of-bounds index, paused contract.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`feat: add set_milestone_deadline entrypoint to activate timeout-refund machinery`

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
title: "Gate withdraw_protocol_fees behind the emergency flag in addition to the pause flag"
labels: type:security, area:protocol-fees, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Gate withdraw_protocol_fees behind the emergency flag in addition to the pause flag

### Description
`withdraw_protocol_fees` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) blocks withdrawal while `DataKey::Paused` is set, but it inspects only the `Paused` flag directly rather than going through the shared `require_not_paused` guard that the rest of the contract uses. Because the guard helper distinguishes the emergency case (`EmergencyActive`) from the ordinary pause case (`ContractPaused`), bypassing it here makes fee withdrawal inconsistent with every other money-flow entrypoint: the error surfaced during an emergency is the generic `ContractPaused`, and any future code path that sets `Emergency` without also setting `Paused` would leave fee withdrawal unexpectedly open.

This issue routes `withdraw_protocol_fees` through the same `require_not_paused` gate, so an active emergency reports `EmergencyActive` and the freeze semantics match the rest of the contract.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Replace the ad-hoc `Paused`-only check in `withdraw_protocol_fees` with the shared `Self::require_not_paused(&env)` guard.
- Confirm `require_not_paused` emits `EmergencyActive` when emergency is active and `ContractPaused` when only paused; keep that distinction observable to callers.
- Preserve admin `require_auth`, the `amount > 0` check, the `InsufficientAccumulatedFees` check, and the typed `SettlementTokenNotConfigured` path.
- Do not change the accumulated-fee accounting or the `fee withdraw` event payload.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-fee-withdraw-emergency-gate`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — swap the inline pause check in `withdraw_protocol_fees` for `require_not_paused`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/protocol_fees.rs`](contracts/escrow/src/test/protocol_fees.rs) — assert withdrawal reverts with `ContractPaused` when paused and `EmergencyActive` when emergency is active, and succeeds once resolved.
  - **Add documentation:** note the emergency gating in the entrypoint doc comment.
  - Include NatSpec-style doc comments (`///`) reflecting both freeze errors.
  - Validate security assumptions: no fee drain during pause or emergency.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: paused, emergency-active, and post-resolution withdrawal.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`fix: gate withdraw_protocol_fees behind the shared pause-and-emergency guard`

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
title: "Reject a zero or contract-self destination address in withdraw_protocol_fees"
labels: type:security, area:protocol-fees, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Reject a zero or contract-self destination address in withdraw_protocol_fees

### Description
`withdraw_protocol_fees(amount, to)` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) decrements `AccumulatedProtocolFees` and then transfers `amount` to the caller-supplied `to` address with no validation of that destination. If `to` is the contract's own address (`env.current_contract_address()`), the fee accounting is reduced while the tokens cycle back into the contract balance — the fees are accounted as withdrawn but remain commingled with escrowed deposits, corrupting the `released + refunded + accumulated_fees <= funded_amount` invariant the release path depends on. There is no guard preventing this self-transfer or other obviously-wrong destinations.

This issue adds destination validation so accumulated fees can only leave the contract to an external address, keeping the accumulated-fee accounting honest.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Reject `to == env.current_contract_address()` with a typed error (e.g. `InvalidWithdrawalDestination`), added append-only to the `Error` enum.
- Perform the destination check before mutating `AccumulatedProtocolFees`, so a rejected withdrawal leaves accounting untouched.
- Keep all existing guards: initialization, pause/emergency, admin auth, positive amount, sufficient accumulated fees, and configured settlement token.
- Document that fee withdrawal must target an external treasury address, not the escrow contract itself.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-fee-withdraw-destination-guard`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the destination guard in `withdraw_protocol_fees`; add the error variant in [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/protocol_fees.rs`](contracts/escrow/src/test/protocol_fees.rs) — assert a self-destination withdrawal reverts and leaves `AccumulatedProtocolFees` unchanged, and a valid external withdrawal succeeds.
  - **Add documentation:** note the external-destination requirement in the entrypoint docs.
  - Include NatSpec-style doc comments (`///`) on the new error path.
  - Validate security assumptions: no self-transfer drains accounting, invariant preserved.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: self-destination, valid external destination, withdrawal exceeding accrued fees.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`fix: reject self-destination in withdraw_protocol_fees to protect fee accounting`

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
title: "Emit a settlement_token_bound event from bind_settlement_token"
labels: type:enhancement, area:events, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Emit a settlement_token_bound event from bind_settlement_token

### Description
`bind_settlement_token` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) is the one-time configuration step that decides which SAC token every escrow deposit, release, refund, and fee withdrawal will move — yet it emits no event. `initialize`, `pause`, `unpause`, the emergency entrypoints, and `set_protocol_fee_bps` all publish events for off-chain indexers, but the single most security-relevant configuration write is invisible to them. Indexers and monitoring dashboards have no way to observe which asset an escrow is settling in, or when that binding happened.

This issue adds a `settlement_token_bound` event so off-chain consumers can track the bound asset and detect (or alarm on) any binding activity.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Publish a `settlement_token_bound` event from `bind_settlement_token` after the write succeeds, carrying `(admin, token, timestamp)`.
- Use a stable topic naming convention consistent with the existing `init` / `protocol_fee_bps` events.
- Do not emit on a rejected bind (unauthorized, uninitialized, or already-bound paths must not publish).
- Keep the event payload free of any non-public data — all fields are already public configuration.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-settlement-token-bound-event`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the `env.events().publish(...)` call in `bind_settlement_token`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/sac_custody.rs`](contracts/escrow/src/test/sac_custody.rs) — assert the event topics and payload on a successful bind, and that no event is emitted on a rejected bind.
  - **Add documentation:** document the event in the `bind_settlement_token` doc comment.
  - Include NatSpec-style doc comments (`///`) describing the event topics and data.
  - Validate security assumptions: event only fires on a successful, authorized bind.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: unauthorized bind, uninitialized bind, already-bound bind (no event).
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`feat: emit settlement_token_bound event for indexer observability`

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
title: "Add a get_milestone reader returning a single milestone by index"
labels: type:feature, area:views, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add a get_milestone reader returning a single milestone by index

### Description
The escrow contract exposes `get_milestones`, which returns the entire `Vec<Milestone>` for a contract from [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs). There is no way to read a single milestone by index. Off-chain callers that only need one milestone's state (amount, funded/released/refunded flags, deadline, work evidence) must fetch and decode the full vector, which is wasteful for contracts approaching `MAX_MILESTONES`, and there is no clean bounds-checked single-item accessor for integrators.

This issue adds a `get_milestone(contract_id, milestone_index)` reader that returns `Option<Milestone>`, returning `None` for an out-of-bounds index and extending milestone TTL consistently with `get_milestones`.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `get_milestone(env, contract_id, milestone_index) -> Option<Milestone>`.
- Panic with `ContractNotFound` if the contract was never allocated; return `None` for an out-of-bounds `milestone_index`.
- Extend the milestones vector TTL on read, matching `get_milestones`.
- Keep the reader auth-free and non-mutating beyond the standard TTL bump.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-get-milestone-reader`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the `get_milestone` reader alongside `get_milestones`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/storage.rs`](contracts/escrow/src/test/storage.rs) — assert a valid index returns the expected milestone, an out-of-bounds index returns `None`, and an unknown contract panics with `ContractNotFound`.
  - **Add documentation:** document the reader and its `None` semantics.
  - Include NatSpec-style doc comments (`///`) describing return values and panics.
  - Validate security assumptions: read-only, correct bounds handling, TTL bump only.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: index 0, last valid index, out-of-bounds index, unknown contract id.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`feat: add get_milestone single-index reader returning Option<Milestone>`

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
title: "Add an is_settlement_token_bound boolean reader for client readiness checks"
labels: type:feature, area:settlement-token, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add an is_settlement_token_bound boolean reader for client readiness checks

### Description
Callers can read the bound asset with `get_settlement_token`, which returns `Option<Address>` from [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs). But integrators that only need to know *whether* an escrow is ready to accept deposits — without caring about the specific token address — must fetch and decode an `Address` they then discard. Because `deposit_funds` currently panics (host `expect`) when no token is bound, a cheap boolean readiness probe is the natural pre-flight check a client SDK wants before submitting a deposit.

This issue adds an `is_settlement_token_bound(env) -> bool` reader that returns `true` exactly when a settlement token is bound.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `is_settlement_token_bound(env) -> bool`, implemented as `Self::read_settlement_token(&env).is_some()`.
- Keep the reader auth-free and fully non-mutating (no TTL writes needed for the simple binding key).
- Document it as the recommended pre-flight check before `deposit_funds`.
- Do not change `get_settlement_token` or the binding write path.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-is-settlement-token-bound`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — add the `is_settlement_token_bound` reader next to `get_settlement_token`.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/sac_custody.rs`](contracts/escrow/src/test/sac_custody.rs) — assert it returns `false` before binding and `true` after.
  - **Add documentation:** describe the reader as a deposit pre-flight check.
  - Include NatSpec-style doc comments (`///`) on the new reader.
  - Validate security assumptions: read-only, no state mutation.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: before bind, after bind.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`feat: add is_settlement_token_bound boolean readiness reader`

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
title: "Assert on-chain token balance conservation across the full deposit-release-refund lifecycle"
labels: type:test, area:custody, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Assert on-chain token balance conservation across the full deposit-release-refund lifecycle

### Description
The escrow contract now moves a real SAC token in `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, and `cancel_contract` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs). The accounting fields (`funded_amount`, `released_amount`, `refunded_amount`, `AccumulatedProtocolFees`) are meant to mirror the contract's actual token balance, but there is no end-to-end test that ties the two together: existing tests check accounting deltas or balances in isolation, not the invariant that the contract's on-chain token balance always equals `funded_amount - released_amount - refunded_amount` plus accrued fees at every step.

This issue adds a lifecycle conservation test that registers a mock SAC and, after each operation, asserts the contract's real token balance matches the derived accounting balance — catching any future drift between the ledger and the books.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Register a mock SAC, bind it, and fund client/freelancer accounts in the test harness.
- Walk a full lifecycle: create, deposit (to Funded), release one milestone (with a non-zero protocol fee), refund another, and assert after each step that contract token balance == `funded - released - refunded` (with accrued fees still held in-contract).
- Add a parallel scenario covering `cancel_contract` returning the full remaining balance.
- Assert the protocol fee remains in the contract until withdrawn, and that withdrawal reduces the balance by exactly the withdrawn amount.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-balance-conservation-lifecycle`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — no production change expected; if the test surfaces a real drift, fix it and note it in the PR.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/accounting_invariants.rs`](contracts/escrow/src/test/accounting_invariants.rs) — implement the balance-vs-accounting assertions across the lifecycle.
  - **Add documentation:** record the conservation invariant in `docs/escrow/`.
  - Include NatSpec-style doc comments (`///`) on any helper introduced.
  - Validate security assumptions: the contract never holds less than it owes nor more than was deposited.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: zero-fee release, cancel-with-balance, partial refund, post-withdrawal balance.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`test: assert token balance conservation across deposit, release, refund, and cancel`

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
title: "Add tests proving is_milestone_overdue boundary behavior at and around the deadline"
labels: type:test, area:milestones, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests proving is_milestone_overdue boundary behavior at and around the deadline

### Description
`is_milestone_overdue` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) documents precise semantics: it returns `false` for a missing contract, missing milestones, an out-of-bounds index, an already-released milestone, or a milestone with no deadline; and for a milestone with a deadline it returns `true` only when `now > deadline` (strictly greater), so exactly at the deadline it returns `false`. These branches and the strict-inequality boundary are not directly exercised, leaving the timeout-refund precondition untested against off-by-one and edge-state errors.

This issue adds focused tests that pin every documented branch of `is_milestone_overdue`, especially the `now == deadline` boundary and the released/no-deadline short-circuits.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Test `now < deadline` (false), `now == deadline` (false), and `now > deadline` (true) using ledger-time control in the harness.
- Test the short-circuits: unknown contract id, contract with no milestones, out-of-bounds index, already-released milestone, and `deadline == None`.
- Use `env.ledger()` time manipulation to set `now_seconds(&env)` deterministically.
- Keep the tests independent of any deadline-setter entrypoint by constructing milestone state directly where needed.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-overdue-boundary`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — no production change expected; fix any genuine off-by-one if found.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/timeout_tests.rs`](contracts/escrow/src/test/timeout_tests.rs) — cover every documented branch and the strict-inequality boundary.
  - **Add documentation:** none required beyond test comments.
  - Include NatSpec-style doc comments (`///`) on any test helper introduced.
  - Validate security assumptions: overdue detection cannot be tripped early at exactly the deadline.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: every branch enumerated above.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`test: pin is_milestone_overdue branch and deadline-boundary behavior`

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
title: "Add tests asserting submit_work_evidence overwrite and 256-byte length bound"
labels: type:test, area:work-evidence, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests asserting submit_work_evidence overwrite and 256-byte length bound

### Description
`submit_work_evidence` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) lets the freelancer attach a deliverable reference to an unreleased, unrefunded milestone on a `Funded` contract, bounded to 256 bytes, overwritable before release, gated by pause/emergency/finalization, and emitting an `evidence` event. Several of these guarantees — the overwrite-before-release behavior, the exact 256-byte boundary (`EvidenceTooLong` at 257, accepted at 256), the freelancer-only authorization, and the `Funded`-state requirement — lack direct tests, so regressions in the evidence path would go unnoticed.

This issue adds tests that pin each documented rule of `submit_work_evidence`, including the byte-length boundary and the overwrite semantics, paired with `get_work_evidence` round-trips.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Assert a successful submission round-trips through `get_work_evidence`.
- Assert overwriting evidence before release replaces the prior value.
- Assert the boundary: 256 bytes accepted, 257 bytes rejected with `EvidenceTooLong`.
- Assert failure paths: non-freelancer caller (`UnauthorizedRole`), non-`Funded` status (`InvalidState`), released/refunded milestone, out-of-bounds index, and paused contract.
- Assert the `evidence` event topics and payload on success.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-work-evidence-bounds`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — no production change expected; fix any genuine boundary bug if found.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/lifecycle.rs`](contracts/escrow/src/test/lifecycle.rs) — implement the evidence submission and bound tests.
  - **Add documentation:** none required beyond test comments.
  - Include NatSpec-style doc comments (`///`) on any test helper introduced.
  - Validate security assumptions: only the freelancer writes evidence, storage bounded at 256 bytes.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: every rule enumerated above.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`test: cover submit_work_evidence overwrite and 256-byte length boundary`

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
title: "Add tests for the protocol-fee floor-division rounding in calculate_protocol_fee"
labels: type:test, area:protocol-fees, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Add tests for the protocol-fee floor-division rounding in calculate_protocol_fee

### Description
`calculate_protocol_fee` in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) computes `amount * fee_bps / 10_000` with integer floor division and panics with `PotentialOverflow` if the multiplication overflows `i128`. The exact rounding behavior — that fees round down, that a tiny `amount` at a small `fee_bps` rounds to zero, and that the overflow guard fires for an `amount` near `i128::MAX` — is the basis for the net-payout math in `release_milestone`, but it is not directly tested. Rounding drift here directly affects how much a freelancer receives versus how much accrues to the protocol.

This issue adds unit tests that pin the floor-division rounding, the zero-fee short-circuit, and the overflow guard of `calculate_protocol_fee`.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Assert `fee_bps == 0` returns `0` without touching the multiplication.
- Assert floor rounding: e.g. an amount/bps combination whose exact product is not divisible by 10_000 rounds down, and a sub-threshold amount yields `0`.
- Assert a representative non-trivial case (e.g. 250 bps of a round amount) yields the exact expected fee.
- Assert the overflow guard panics with `PotentialOverflow` for an `amount` near `i128::MAX` at a non-zero `fee_bps`.
- Confirm the net payout (`gross - fee`) is never negative for valid inputs.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-protocol-fee-rounding`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — no production change expected; fix any genuine rounding bug if found.
  - **Write comprehensive tests in:** [`contracts/escrow/src/protocol_fees_test.rs`](contracts/escrow/src/protocol_fees_test.rs) — implement the rounding, zero-fee, and overflow tests.
  - **Add documentation:** confirm the floor-rounding contract in the function doc comment.
  - Include NatSpec-style doc comments (`///`) on any test helper introduced.
  - Validate security assumptions: fees never exceed the gross amount, overflow always trapped.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: zero bps, sub-threshold amount, indivisible product, near-`i128::MAX` overflow.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`test: cover calculate_protocol_fee floor rounding, zero-fee, and overflow guard`

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
title: "Collapse the triplicated DisputeResolution, raise_dispute, and resolve_dispute definitions"
labels: type:refactor, area:dispute, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Collapse the triplicated DisputeResolution, raise_dispute, and resolve_dispute definitions

### Description
[`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) defines `DisputeResolution` three separate times (two as `Split(i128, i128)`, plus a struct-based `Split(DisputeSplit)` in `types.rs`), and contains three `#[contractimpl] impl Escrow` blocks each redefining `raise_dispute` and two of them redefining `resolve_dispute`, alongside duplicate `resolution_payouts`, `final_status_after_resolution`, and `code()` implementations. The copies disagree: some reference a nonexistent `EscrowContractData`/`total_deposited`, one omits the arbiter and state checks entirely, and the `Split` arities differ. This cannot all compile as written and is a maintenance hazard — fixes to dispute logic must currently be made in several inconsistent places.

This issue collapses the dispute module to a single canonical `DisputeResolution`, one `raise_dispute`, one `resolve_dispute`, and one copy each of `resolution_payouts`, `final_status_after_resolution`, and `code()`, all consistent with the `types.rs` definitions and the lib.rs entrypoints.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Pick the canonical `DisputeResolution` shape (the `types.rs` `Split(DisputeSplit)` form) and delete the divergent `Split(i128, i128)` copies.
- Remove the duplicate `#[contractimpl] impl Escrow` blocks so only one `raise_dispute` and one `resolve_dispute` remain (the lib.rs ones, or a single delegating module impl — choose one and document it).
- Delete references to the nonexistent `EscrowContractData`/`total_deposited` in dispute math; use the real `Contract.funded_amount`.
- Keep a single `resolution_payouts`, `final_status_after_resolution`, and `code()` consistent with the canonical type.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-dedupe-dispute-module`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — keep the canonical dispute entrypoints; trim [`contracts/escrow/src/dispute.rs`](contracts/escrow/src/dispute.rs) to the single set of helpers and the canonical enum.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/dispute.rs`](contracts/escrow/src/test/dispute.rs) — assert all four resolution variants still resolve correctly through the single entrypoint after dedup.
  - **Add documentation:** note which module owns dispute logic.
  - Include NatSpec-style doc comments (`///`) on the surviving definitions.
  - Validate security assumptions: arbiter auth, Disputed-state gate, and conservation checks survive on the single path.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: each resolution variant, non-arbiter caller, non-Disputed contract.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`refactor: collapse triplicated dispute definitions into a single canonical module`

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
title: "Remove the second create_contract definition and duplicated body in create_contract.rs"
labels: type:refactor, area:create-contract, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Remove the second create_contract definition and duplicated body in create_contract.rs

### Description
[`contracts/escrow/src/create_contract.rs`](contracts/escrow/src/create_contract.rs) contains a complete `pub fn create_contract(...)` inside a `#[contractimpl] impl Escrow` block, and then — after that function closes — a second, dangling copy of nearly the entire creation body (milestone validation, governed-cap lookup, contract construction, milestone vector build, next-id write, event emit) sitting outside any function, followed by a free `next_contract_id` helper. The two copies disagree (one uses `amount_validation::validate_milestone_amounts`, the other an inline fold; one omits the `total_deposited`/`reputation_issued` fields the `Contract` struct requires). As written this file cannot compile, and the divergence means creation invariants are defined twice and inconsistently.

This issue reduces the module to a single coherent `create_contract` path that matches the `Contract` struct in `types.rs` and the lib.rs entrypoint signature.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Keep exactly one creation implementation; delete the dangling duplicated body.
- Ensure the surviving path constructs `Contract` with all required fields (`total_deposited`, `reputation_issued`) and uses one consistent milestone-validation routine.
- Preserve the `MAX_MILESTONES` cap, the governed total-escrow cap behavior, the arbiter requirement per `ReleaseAuthorization`, and the `created` event.
- Keep `next_contract_id` as a single helper with its collision/overflow guards intact.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-dedupe-create-contract`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/create_contract.rs`](contracts/escrow/src/create_contract.rs) — collapse to one implementation; align field construction with [`contracts/escrow/src/types.rs`](contracts/escrow/src/types.rs).
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/create_contract.rs`](contracts/escrow/src/test/create_contract.rs) — assert creation succeeds with correct initial fields and that the cap/arbiter/milestone guards still reject as before.
  - **Add documentation:** note the single canonical creation path.
  - Include NatSpec-style doc comments (`///`) on the surviving function.
  - Validate security assumptions: caps and arbiter rules enforced exactly once, no id reuse.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: empty milestones, over-cap total, missing arbiter, invalid arbiter, id collision.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`refactor: remove duplicated create_contract body and converge on one creation path`

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
title: "Document the SAC custody and settlement-token lifecycle for the escrow contract"
labels: type:docs, area:custody, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN
assignees: ''
---

## Document the SAC custody and settlement-token lifecycle for the escrow contract

### Description
The escrow contract custodies a single Stellar Asset Contract token: `bind_settlement_token` selects it, `deposit_funds` pulls it into the contract, `release_milestone` pays the freelancer net of the protocol fee, `refund_unreleased_milestones` and `cancel_contract` return it to the client, and `withdraw_protocol_fees` drains accrued fees — all in [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs). There is no single document that explains this custody model: which entrypoint moves funds in which direction, that the contract address itself holds the balance, that accrued fees stay commingled until withdrawn, and what the invariant `contract balance == funded - released - refunded + accrued_fees` means for integrators and auditors.

This issue writes that custody-and-settlement document so integrators understand the on-chain money flow and the one-token-per-escrow assumption.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Document the settlement-token binding step and its write-once intent.
- Map each fund-moving entrypoint to its source, destination, and amount (deposit, release net-of-fee, refund, cancel, fee withdrawal).
- State the balance/accounting invariant and where accrued protocol fees sit until withdrawn.
- Include a sequence diagram of a full lifecycle (bind, deposit, release, withdraw fees) and call out the unconfigured-token failure mode.
- Do not document fields or flows that do not exist in the current code; cross-check every claim against `lib.rs`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-sac-custody-lifecycle`
- Implement changes
  - **Write code in:** [`contracts/escrow/src/lib.rs`](contracts/escrow/src/lib.rs) — doc-comment cross-references only; link the new doc from the relevant entrypoints' `///` comments.
  - **Write comprehensive tests in:** [`contracts/escrow/src/test/sac_custody.rs`](contracts/escrow/src/test/sac_custody.rs) — add or extend a test that exercises the documented happy-path flow so the doc stays honest.
  - **Add documentation:** create `docs/escrow/sac-custody.md` describing the custody and settlement-token lifecycle.
  - Include NatSpec-style doc comments (`///`) where entrypoint docs reference the new file.
  - Validate security assumptions: the doc accurately reflects fund directions and the balance invariant.
- Test and commit

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, and `cargo test`.
- Cover edge cases and failure paths: ensure the documented flow matches a passing end-to-end test.
- Include the full `cargo test` output and a short security notes section in the PR description.

### Example commit message
`docs: document the SAC custody and settlement-token lifecycle for the escrow contract`

### Guidelines
- **Minimum 95 percent test coverage** for impacted modules.
- Clear, reviewer-focused documentation.
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord for questions, reviews, and faster merges:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — if this issue and the maintainers helped you ship, we'd be grateful for a **5-star rating**. Clear questions in Discord and tidy, well-tested PRs are the fastest path to a merge and a reward.
