---
type: Feature
title: "Persist a DisputeRecord under a dedicated DataKey with raiser, reason, and resolution"
labels: type:feature, area:dispute, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Persist a DisputeRecord under a dedicated DataKey with raiser, reason, and resolution

### Description
`raise_dispute` and `resolve_dispute` in `lib.rs` only flip `Contract.status` to `Disputed` and back through `final_status_after_resolution`, so nothing on-chain records who raised the dispute, when, or which `DisputeResolution` variant was applied. The `DataKey` enum in `types.rs` has no dispute variant at all. Add a persisted `DisputeRecord` plus a `get_dispute_record` reader.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add `DataKey::Dispute(u32)` and a `DisputeRecord { raiser, raised_at_ledger, resolution: Option<DisputeResolution>, resolved_at_ledger: Option<u32> }` struct in `types.rs`.
- Write the record in `raise_dispute` and complete it in `resolve_dispute`, keeping `resolution_payouts` untouched.
- Extend the persistent TTL for the new key alongside `ttl::extend_contract_ttl`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-dispute-record`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/dispute.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(dispute): persist DisputeRecord and expose get_dispute_record`

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
title: "Add a get_version entrypoint reporting contract and ContractSummary schema versions"
labels: type:feature, area:versioning, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add a get_version entrypoint reporting contract and ContractSummary schema versions

### Description
The crate carries `version = "0.1.0"` in `Cargo.toml` and `CONTRACT_SUMMARY_SCHEMA_VERSION` in `types.rs`, but neither value is readable on-chain. Integrators calling `get_contract_summary` cannot tell which schema they are decoding without out-of-band knowledge. Add a `get_version` entrypoint returning both.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Return a `VersionInfo { contract_version: u32, summary_schema_version: u32 }` derived from compile-time constants.
- Keep the entrypoint callable while paused and during emergency, since it reads no contract state.
- Assert in tests that `summary_schema_version` matches `CONTRACT_SUMMARY_SCHEMA_VERSION` and the value embedded in `get_contract_summary`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-get-version`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/summary.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(versioning): add get_version entrypoint for contract and schema versions`

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
title: "Add a batched get_contract_summaries reader accepting a list of contract ids"
labels: type:feature, area:indexer, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add a batched get_contract_summaries reader accepting a list of contract ids

### Description
`get_contract_summary` in `lib.rs` returns one `ContractSummary` per call, so a front-end rendering a dashboard of ten escrows issues ten simulation calls. Add a bounded batch reader that accepts `Vec<u32>` and returns the corresponding summaries in the same order.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Cap the input length (reuse a constant of the same order as `MAX_MILESTONES`) and error with `ContractNotFound` semantics or skip-and-report for unknown ids — pick one and document it.
- Reuse the existing summary construction path rather than duplicating the `MilestoneSummary` mapping.
- Call `ttl::extend_contract_and_milestones_ttl` consistently with the single-id reader.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-batch-summaries`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/summary.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(indexer): add batched get_contract_summaries reader`

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
title: "Allow a third-party payer to fund an escrow with recorded client consent"
labels: type:feature, area:funding, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Allow a third-party payer to fund an escrow with recorded client consent

### Description
`deposit_funds` binds the depositor to `Contract.client` and moves tokens from the caller via `token::Client::transfer`. Agencies and DAOs commonly want a treasury address to pay on the client's behalf. Add an opt-in payer authorization so a designated funder can deposit while the client remains the refund destination.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add an `authorize_funder(contract_id, client, funder)` entrypoint requiring `client.require_auth()` and storing the approved funder.
- In `deposit_funds`, accept either the client or the stored funder as caller; refunds and cancellation payouts must still target `Contract.client`.
- Emit an event on funder authorization and reject a funder equal to the freelancer or arbiter.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b feature/contracts-third-party-funder`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/deposit.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(funding): allow authorized third-party funders to deposit`

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
title: "Collapse bind_settlement_token and set_settlement_token into one guarded binder"
labels: type:enhancement, area:settlement-token, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Collapse bind_settlement_token and set_settlement_token into one guarded binder

### Description
`lib.rs` exposes both `bind_settlement_token` and `set_settlement_token` writing the same `DataKey::SettlementToken` entry, with only one of them carrying the `SettlementTokenAlreadyBound` guard and the `settlement_token_bound` event. Two write paths to the custody token is a footgun: an admin can silently swap the token that holds live escrow funds. Consolidate to one entrypoint.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Keep `bind_settlement_token` as the single writer with admin auth, the already-bound guard, and event emission.
- Either delete `set_settlement_token` or make it a deprecated thin delegate that inherits every guard, documented as such.
- Add a test asserting the second bind attempt fails with `SettlementTokenAlreadyBound` regardless of which entrypoint is called.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-single-token-binder`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/sac_custody.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`refactor(settlement-token): collapse dual token binders into one guarded path`

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
title: "Replace the Windows-only CI workflow with a wasm32 release build and size budget"
labels: type:enhancement, area:ci, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Replace the Windows-only CI workflow with a wasm32 release build and size budget

### Description
`.github/workflows/ci.yml` runs on `windows-latest`, verifies the MSVC linker, and builds only the host target. Nothing in CI ever compiles the escrow crate to `wasm32-unknown-unknown`, which is the only target that matters for deployment, and no job bounds the resulting `.wasm` size against the ledger entry limit.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Move the job to `ubuntu-latest`, add the `wasm32-unknown-unknown` target, and run `cargo build --target wasm32-unknown-unknown --release`.
- Add a step that fails when the emitted `escrow.wasm` exceeds a documented byte budget.
- Keep `cargo test --workspace` on the host target and pin the toolchain via `rust-toolchain.toml`.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b enhancement/contracts-wasm-ci`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/performance.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`ci: build wasm32 release target and enforce a wasm size budget`

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
title: "Auto-engage emergency pause when the accounting invariant guard trips"
labels: type:security, area:circuit-breaker, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Auto-engage emergency pause when the accounting invariant guard trips

### Description
`EscrowError::AccountingInvariantViolated` is raised when `funded_amount`, `released_amount`, and `refunded_amount` fail to reconcile, but the panic only aborts the single transaction — every other contract remains fully operational under the same possibly-corrupt code path. Wire the invariant guard into the existing emergency machinery so a detected violation trips the global breaker.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Introduce a helper that writes `DataKey::Emergency` and emits the emergency event before panicking with `AccountingInvariantViolated`.
- Apply it at every reconciliation site in release, refund, cancel, and dispute payout paths.
- Recovery must still require the admin-gated `resolve_emergency`; auto-trip must never be self-clearing.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-invariant-circuit-breaker`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/accounting_invariants.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(security): auto-engage emergency pause on accounting invariant violation`

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
title: "Add a configurable treasury destination allowlist for protocol fee withdrawals"
labels: type:security, area:treasury, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add a configurable treasury destination allowlist for protocol fee withdrawals

### Description
`withdraw_protocol_fees(env, amount, to)` lets the admin send accumulated fees to any arbitrary address, so a single compromised admin key drains `DataKey::AccumulatedProtocolFees` in one transaction. Constrain withdrawals to a pre-registered treasury destination set that is itself changed through the governance path.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Store an allowlist under a new `DataKey` variant, mutated only by the governance admin used in `governance.rs`.
- Reject `withdraw_protocol_fees` with `UnauthorizedRole` when `to` is not on the allowlist, and keep the existing `InsufficientAccumulatedFees` check ordering.
- Emit an event on allowlist mutation so indexers can audit treasury changes.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-treasury-allowlist`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/protocol_fees.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(treasury): restrict fee withdrawals to an allowlisted destination set`

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
title: "Probe the settlement token contract before binding it as escrow custody asset"
labels: type:security, area:settlement-token, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Probe the settlement token contract before binding it as escrow custody asset

### Description
`bind_settlement_token` accepts any `Address` and stores it under `DataKey::SettlementToken`; every later `token::Client::new` call in `deposit_funds`, `release_milestone`, and `cancel_contract` trusts it blindly. Binding a non-token address bricks all custody flows, and binding a hostile contract hands it control of the transfer path.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Perform a read-only probe (for example `token::Client::balance` against `env.current_contract_address()`) inside `bind_settlement_token` and reject addresses that fail it.
- Explicitly reject `env.current_contract_address()` and the escrow admin address as settlement tokens.
- Document that reentrancy from a malicious token is mitigated by state-before-transfer ordering, and add a test using a mock token contract.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b security/contracts-token-probe`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/sac_custody.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`feat(security): probe and validate the settlement token before binding`

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
title: "Add direct unit tests for the ttl helper functions against evicted entries"
labels: type:test, area:ttl, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add direct unit tests for the ttl helper functions against evicted entries

### Description
`ttl.rs` exports `store_with_ttl`, `read_if_live`, `extend_if_below_threshold`, `remove_transient`, and `has_transient`, yet the existing suites exercise them only indirectly through approval and migration flows. The generic helpers deserve targeted coverage against a ledger advanced past the stored expiry.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Use `env.ledger().set_sequence_number` to advance past `compute_expiry` and assert `read_if_live` returns `None` while `has_transient` returns `false`.
- Assert `extend_if_below_threshold` returns `true` only when the remaining TTL is under the threshold and is a no-op otherwise.
- Cover `remove_transient` idempotence on an already-absent key.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-ttl-helpers`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/ttl_tests.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`test(ttl): add direct coverage for ttl helper eviction semantics`

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
title: "Add overflow-boundary unit tests for accumulate_amounts and the safe arithmetic helpers"
labels: type:test, area:amount-validation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add overflow-boundary unit tests for accumulate_amounts and the safe arithmetic helpers

### Description
`amount_validation.rs` exposes `safe_add_amounts`, `safe_subtract_amounts`, and `accumulate_amounts` as the crate's arithmetic safety net, and all three are re-exported from `lib.rs`. Their behaviour at `i128::MAX`, `i128::MIN`, and on empty iterators is not pinned by tests, so a future refactor could silently reintroduce wrapping.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Assert `safe_add_amounts(i128::MAX, 1)` and `safe_subtract_amounts(i128::MIN, 1)` both return `None`.
- Cover `accumulate_amounts` on an empty iterator, a single element, and a sequence that overflows midway.
- Add a case proving `validate_amount_array` rejects a slice whose sum exceeds `MAX_SINGLE_AMOUNT_STROOPS` even when each element passes individually.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-amount-overflow-bounds`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/input_sanitization_amounts.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`test(amount-validation): pin overflow boundaries for safe arithmetic helpers`

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
title: "Assert on-chain token balances for the cancel_contract client refund transfer"
labels: type:test, area:cancellation, stack:soroban, stack:rust, priority:high, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Assert on-chain token balances for the cancel_contract client refund transfer

### Description
`cancel_contract` performs a real `token::Client::new(&env, &token).transfer` back to the client, but the cancellation suite checks status transitions and accounting counters rather than actual token balances. A regression that updates `refunded_amount` without moving tokens would pass today.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Register a Stellar Asset Contract with `StellarAssetClient`, fund the client, deposit, then cancel and compare pre/post balances for client and `env.current_contract_address()`.
- Cover the zero-funded cancellation case where no transfer should occur at all.
- Assert the escrow's residual balance equals the sum of still-escrowed funds across other contracts in the same test env.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-cancel-balance-assertions`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/cancel_contract.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`test(cancellation): assert real token balance movement on cancel refund`

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
title: "Cover pending reputation credit accrual and drain across multiple completed contracts"
labels: type:test, area:reputation, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Cover pending reputation credit accrual and drain across multiple completed contracts

### Description
`get_pending_reputation_credits` reads `DataKey::PendingReputationCredits(Address)`, which `issue_reputation` mutates as contracts complete. The multi-contract accumulation path — one freelancer finishing several escrows before any rating is issued — has no test, so off-by-one accrual would go unnoticed.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Complete three escrows for the same freelancer, assert the pending credit count after each, then issue reputation and assert the drain plus `Reputation.completed_contracts`.
- Assert `ReputationAlreadyIssued` on a second `issue_reputation` call for the same contract id and that the credit balance is unchanged.
- Include a case where a contract ends `Refunded` and must not accrue any credit.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b test/contracts-pending-reputation-credits`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/reputation.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`test(reputation): cover pending credit accrual and drain across contracts`

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
title: "Merge the duplicate participant index pagination test modules into one"
labels: type:refactor, area:tests, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Merge the duplicate participant index pagination test modules into one

### Description
`contracts/escrow/src/test/` contains both `pagination_participant_index.rs` and `participant_index_pagination.rs`, two modules covering the same participant-index pagination surface under transposed names. Duplicated suites drift apart and double the maintenance cost of every pagination change.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Diff both files, keep the union of unique assertions, and delete the redundant module along with its `mod` declaration in `test/mod.rs`.
- Preserve every distinct edge case (empty page, offset past the end, limit larger than the index) in the surviving module.
- Confirm `ttl::extend_participant_contract_index_ttl` remains exercised after the merge.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-merge-pagination-tests`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/participant_index_pagination.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`refactor(tests): merge duplicate participant index pagination modules`

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
title: "Drop the placeholder hello entrypoint from the deployed escrow interface"
labels: type:refactor, area:api-surface, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Drop the placeholder hello entrypoint from the deployed escrow interface

### Description
`Escrow::hello(_env, to: Symbol) -> Symbol` in `lib.rs` is a scaffolding leftover from the Soroban template, yet it ships in the contract ABI and is covered by `test/hello.rs`. It costs wasm bytes, pollutes the generated client, and suggests to integrators that it is a supported endpoint.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Remove `hello` and its `test/hello.rs` module plus the `mod` declaration in `test/mod.rs`.
- If a liveness probe is genuinely wanted, replace it with a documented `get_version`-style reader rather than an echo function.
- Update the README and any ABI reference so the removed entrypoint no longer appears.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-remove-hello`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/mod.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`refactor(api): remove the placeholder hello entrypoint from the escrow ABI`

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
title: "Fold test_helpers_legacy into a single shared escrow test fixture builder"
labels: type:refactor, area:test-fixtures, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Fold test_helpers_legacy into a single shared escrow test fixture builder

### Description
`contracts/escrow/src/test_helpers_legacy.rs` sits beside a dozen top-level `test_*.rs` modules and the `test/` directory, each rebuilding its own `Env`, admin, participants, and milestone vectors. The name itself flags the file as superseded, but nothing replaced it. Consolidate into one fixture builder that every suite consumes.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Provide a builder exposing `with_admin`, `with_participants`, `with_milestones`, `with_settlement_token`, and `funded()` shortcuts returning a ready escrow id.
- Migrate at least the deposit, release, refund, and dispute suites onto it and delete `test_helpers_legacy.rs`.
- Keep the fixture behind `#[cfg(test)]` so it never contributes to the wasm build.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b refactor/contracts-test-fixture-builder`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/mod.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`refactor(tests): replace legacy helpers with a shared escrow fixture builder`

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
title: "Add a CONTRIBUTING guide covering the Soroban toolchain setup and PR checklist"
labels: type:docs, area:contributing, stack:soroban, stack:rust, priority:medium, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add a CONTRIBUTING guide covering the Soroban toolchain setup and PR checklist

### Description
The repository has no CONTRIBUTING file, so new contributors must infer the toolchain from `Cargo.toml` (`rust-version = "1.75"`, `soroban-sdk = "22.0"`) and the CI workflow. Document the exact setup, the local verification commands, and what a reviewable PR against `contracts/escrow` must contain.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Cover installing the pinned Rust toolchain, the `wasm32-unknown-unknown` target, and the Stellar CLI, plus running `cargo fmt --all -- --check`, `cargo clippy`, and `cargo test --workspace`.
- Document where new tests belong (`contracts/escrow/src/test/`) and the conventional-commit prefixes used in history.
- Include the review checklist: auth guards, accounting invariants, event emission, TTL extension, and negative-path tests.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-contributing-guide`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/mod.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`docs: add CONTRIBUTING guide with toolchain setup and PR checklist`

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
title: "Add a crate-level rustdoc module map for the escrow source tree"
labels: type:docs, area:rustdoc, stack:soroban, stack:rust, priority:low, MAYBE REWARDED, GRANTFOX OSS, OFFICIAL CAMPAIGN, Official Campaign | FWC26
assignees: ''
---
## Add a crate-level rustdoc module map for the escrow source tree

### Description
`lib.rs` opens with `#![no_std]` and two dozen clippy allows, then declares `amount_validation`, `approvals`, `deposit`, `finalize`, `migration`, `ttl`, `types`, `utils`, `create_contract`, `dispute`, and `governance` with no crate-level `//!` documentation explaining what each owns. A reader cannot tell that money movement lives in `lib.rs` while `deposit.rs` and `refund.rs` hold supporting logic.

### Requirements and context
- **Repository scope:** Talenttrust/Talenttrust-Contracts only.
- Add a `//!` crate doc listing every module with a one-line responsibility and the storage keys it owns.
- Add module-level `//!` headers to `ttl.rs`, `amount_validation.rs`, `approvals.rs`, `dispute.rs`, and `governance.rs`.
- Ensure `cargo doc --no-deps` builds without warnings and link the generated map from the README.

### Suggested execution
- Fork the repo and create a branch
- `git checkout -b docs/contracts-module-map`
- **Write code in:** `contracts/escrow/src/lib.rs`
- **Write comprehensive tests in:** `contracts/escrow/src/test/mod.rs`
- **Add documentation:** README / docs
- Include NatSpec-style `///` comments

### Test and commit
- Run `cargo fmt --all -- --check`, `cargo build`, `cargo test`
- Cover edge cases and failure paths

### Example commit message
`docs(rustdoc): add crate-level module map for the escrow contract`

### Guidelines
- Minimum 95 percent test coverage for impacted modules
- Clear documentation
- **Timeframe: 96 hours.**

### Community & contribution rewards
- 💬 **Join the TalentTrust community on Discord:** https://discord.gg/WqnGpcPx
- ⭐ This is a **GrantFox OSS / Official Campaign** task and **may be rewarded**. When your PR is merged you'll be prompted to rate the project — a **5-star rating** is much appreciated.
