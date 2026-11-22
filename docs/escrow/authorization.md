# Escrow Authorization Matrix

This document is the security reference for authorization in the TalentTrust escrow contract. It covers every state-changing entrypoint exposed by the `#[contractimpl]` block in `contracts/escrow/src/lib.rs`, together with the signer checked by `require_auth()`, the authorization role, the pause category, and per-escrow freeze behavior. Read-only entrypoints are intentionally omitted.

**Source of truth:** `contracts/escrow/src/lib.rs` for implementation and `contracts/escrow/src/test.rs` for MockAuth and gate tests. If this document and the implementation disagree, the implementation wins.

---

## 1. Authority model

The contract recognizes the following authority categories. The **Role** column in the matrix uses these names.

| Category | Source of authority | Notes |
|----------|--------------------|-------|
| `Admin` | `DataKey::Admin`, written once by `initialize` | Stored admin authorizes protocol administration. The admin implicitly holds both `Pauser` and `Resolver` via `holds_role`, so an explicit role grant is not required. |
| `Pending admin` | `DataKey::PendingAdmin` | A nominee proposed by the current admin. Only the nominee may call `accept_admin`; no timelock is enforced by the contract. |
| `Pauser` | Admin or `DataKey::RoleMember(Role::Pauser, account)` | Granted or revoked by `grant_role` / `revoke_role`. May operate the circuit breaker only. |
| `Resolver` | Admin or `DataKey::RoleMember(Role::Resolver, account)` | May call `resolve_dispute`, but never when the resolver is a party to that escrow. For high-value escrows, the panel path is mandatory. |
| `Panel member` | Current `Panel.members` set by `set_panel` | May vote on high-value disputes. A party to the escrow may not vote, and only votes from current members are tallied. |
| `Party` | `EscrowContract.client` or `EscrowContract.freelancer` | Per-escrow authority. Identity is read from stored escrow state, not from an arbitrary caller argument, except where the entrypoint explicitly takes and validates a party address. |
| `Anyone` | No signature required | Permissionless maintenance, expiry, or public governance actions. State changes are constrained by status, time, and storage predicates rather than by a signer. |

A "funder" is an address named by the caller of `deposit_from`. The funder must authorize the deposit; the caller otherwise has no authority. An "acceptor" or "proposer" in a mutual action must be one of the two stored parties and must authorize the call.

---

## 2. Pause and freeze semantics

### 2.1 Pause categories

`contracts/escrow/src/lib.rs` defines six pause bits:

| Constant | Value | Gates |
|----------|------:|-------|
| `PAUSE_CREATE` | 1 | Creation, setup, and scope changes |
| `PAUSE_DEPOSIT` | 2 | Funding inflows |
| `PAUSE_RELEASE` | 4 | Submission, claims, payouts, bonuses, delivery metadata |
| `PAUSE_DISPUTE` | 8 | Opening disputes |
| `PAUSE_CANCEL` | 16 | Cancellation, resignation, expiry, and mutual settlement |
| `PAUSE_REPUTATION` | 32 | Ratings and rating voiding |
| `PAUSE_ALL` | 63 | All six categories |

`require_not_paused(op)` fails with `Paused` when `pause_flags() & op != 0`. `set_paused(caller, true)` writes `PAUSE_ALL`; `set_paused(caller, false)` clears all flags. `set_pause_flags(caller, flags)` writes an exact subset and rejects unknown bits. A legacy `Paused` value of `true` with no stored `PauseFlags` is interpreted as `PAUSE_ALL`.

Admin rotation, pause control, fee configuration, role management, upgrades, sweep, freeze control, dispute resolution, panel voting, emergency withdrawal, storage migration, TTL extension, and compaction are intentionally **not** pause-gated. They remain available as recovery or maintenance actions. `execute_upgrade` and admin rotation are explicitly allowed while paused.

For gated party flows the pause check runs before `require_auth()`. `void_rating` is the exception: it authenticates the admin first and then checks `PAUSE_REPUTATION`.

### 2.2 Per-escrow freeze

`freeze_escrow(id, reason)` sets `DataKey::Frozen(id)`. `unfreeze_escrow(id)` clears it. When an entrypoint marked **Blocked** below calls `require_not_frozen(id)`, the call fails with `EscrowFrozen` before authentication and before any token movement.

Freeze is limited to party-initiated escrow mutations. Administrative recovery (`emergency_withdraw`), dispute resolution (`resolve_dispute` and `vote_resolution`), rating voiding, and maintenance operations deliberately do not check the frozen flag. `compact_escrow` is permissionless but does check it.

---

## 3. Complete state-changing authorization matrix

Every mutating public entrypoint in `contracts/escrow/src/lib.rs` appears below. `Source` in the signer column means the address is read from stored state rather than supplied as an argument.

| Entrypoint | Required `require_auth` subject | Role | Pause category | Freeze behavior | Key preconditions |
|------------|--------------------------------|------|----------------|-----------------|-------------------|
| `initialize(admin)` | `admin` argument | Admin | None | N/A | Admin slot must be empty; initializes admin, pause state, and id counter |
| `propose_admin(new_admin)` | Stored admin | Admin | None | N/A | Contract initialized; `new_admin` must differ from current admin; overwrites any pending nominee |
| `accept_admin()` | Pending nominee from storage | Pending admin | None | N/A | A pending nomination must exist; no timelock in contract code |
| `cancel_admin_proposal()` | Stored admin | Admin | None | N/A | A pending nomination must exist |
| `set_paused(caller, paused)` | `caller` argument | Admin or Pauser | None; writes the gate | N/A | `caller` must hold `Pauser` or be admin; `true` writes `PAUSE_ALL`, `false` clears flags |
| `set_pause_flags(caller, flags)` | `caller` argument | Admin or Pauser | None; writes the gate | N/A | Same role gate; `flags & !PAUSE_ALL` must be zero; starts or clears the emergency-exit clock |
| `set_fee(fee_bps, treasury)` | Stored admin | Admin | None | N/A | `fee_bps <= MAX_FEE_BPS` (500); treasury must not be this contract |
| `emergency_withdraw(id, caller)` | `caller` argument | Party | None; requires an active pause | Allowed while frozen | Caller must be stored client or freelancer; pause must have been continuous for at least 30 days; status must be `Created`, `Funded`, or `Disputed` |
| `sweep_surplus(token, to)` | Stored admin | Admin | None | N/A | Token balance must exceed aggregate liability |
| `set_payout_address(id, payout)` | Stored freelancer | Party | `PAUSE_CREATE` | Blocked | Status `Created` or `Funded`; payout must not be the contract or client |
| `freeze_escrow(id, reason)` | Stored admin | Admin | None | N/A | Escrow must exist and not already be frozen |
| `unfreeze_escrow(id)` | Stored admin | Admin | None | N/A | Escrow must exist and currently be frozen |
| `create_contract(client, freelancer, token, amounts)` | `client` argument | Party (client) | `PAUSE_CREATE` | N/A | Admin initialized; parties distinct and not the contract or admin; 1..=20 valid milestone amounts |
| `create_contract_with_review(client, freelancer, token, amounts, window)` | `client` argument | Party (client) | `PAUSE_CREATE` | N/A | Same creation checks; review window within 1 hour..=30 days |
| `create_and_fund(client, freelancer, token, amounts)` | `client` argument | Party (client) | `PAUSE_CREATE` and `PAUSE_DEPOSIT` | N/A | Same creation checks; client transfers the full total in the same call |
| `create_contract_sequential(client, freelancer, token, amounts)` | `client` argument | Party (client) | `PAUSE_CREATE` | N/A | Same creation checks; marks the escrow as strictly ordered |
| `create_stream(client, freelancer, token, amount, start, end)` | `client` argument | Party (client) | `PAUSE_CREATE` | N/A | Same creation checks; `start < end` and duration at least 1 hour |
| `set_team_split(id, payees)` | Stored freelancer | Party (freelancer) | `PAUSE_CREATE` | Blocked | Status `Created`, zero funded; 1..=5 unique payees, nonzero bps summing to 10,000 |
| `deposit_funds(id, amount)` | Stored client | Party (client) | `PAUSE_DEPOSIT` | Blocked | Status `Created`; funding deadline not passed; amount positive and does not overfund |
| `deposit_from(id, funder, amount)` | `funder` argument | Anyone (funder signs) | `PAUSE_DEPOSIT` | Blocked | Same deposit checks; at most 10 distinct funders per escrow |
| `submit_milestone(id, index)` | Stored freelancer | Party (freelancer) | `PAUSE_RELEASE` | Blocked | Non-stream escrow, status `Funded`; milestone unreleased and its remaining amount is covered |
| `claim_milestone(id, index)` | Stored freelancer | Party (freelancer) | `PAUSE_RELEASE` | Blocked | Non-stream escrow; milestone was submitted and review window elapsed; status `Funded` or `Cancelled` |
| `release_milestone(id, index)` | Stored client | Party (client) | `PAUSE_RELEASE` | Blocked | Non-stream escrow, status `Created` or `Funded`; milestone unreleased; held funds cover it; sequential order respected |
| `release_partial(id, index, amount)` | Stored client | Party (client) | `PAUSE_RELEASE` | Blocked | Same release checks; `0 < amount <= remaining` on the milestone |
| `release_milestones(id, indices)` | Stored client | Party (client) | `PAUSE_RELEASE` | Blocked | Same release checks; 1..=20 unique indices; held funds cover the batch; sequential order respected |
| `pay_bonus(id, amount)` | Stored client | Party (client) | `PAUSE_RELEASE` | Blocked | Status `Funded` or `Completed`; amount positive; direct client-to-payee transfer |
| `withdraw_vested(id)` | Stored freelancer | Party (freelancer) | `PAUSE_RELEASE` | Blocked | Stream escrow, status `Funded`, not stopped; newly vested amount is positive |
| `stop_stream(id)` | Stored client | Party (client) | `PAUSE_CANCEL` | Blocked | Stream escrow, status `Funded`, not already stopped |
| `open_dispute(id, caller)` | `caller` argument | Party | `PAUSE_DISPUTE` | Blocked | Caller is stored client or freelancer; status `Created` or `Funded`; caller transfers the dispute bond |
| `resolve_dispute(caller, id, bps, bond_to_opener)` | `caller` argument | Admin or Resolver | None | Allowed while frozen | Caller holds `Resolver` or is admin and is not a party; status `Disputed`; total below panel threshold; `bps <= 10,000` |
| `set_panel(members, threshold)` | Stored admin | Admin | None | N/A | `1 <= threshold <= members.len() <= 7`; no duplicate members |
| `vote_resolution(member, id, bps)` | `member` argument | Panel member | None | Allowed while frozen | Status `Disputed`; member is on the current panel and not a party; `bps <= 10,000`; matching votes reach threshold to settle |
| `propose_change_order(id, amounts)` | Stored client | Party (client) | `PAUSE_CREATE` | Blocked | Status `Created`, `Funded`, or `Completed`; added milestones fit `MAX_MILESTONES` and amount limits |
| `accept_change_order(id)` | Stored freelancer | Party (freelancer) | `PAUSE_CREATE` | Blocked | Pending change order exists; status allows acceptance; reopens escrow as `Created` |
| `reject_change_order(id)` | Stored freelancer | Party (freelancer) | None | Allowed while frozen | Pending change order exists |
| `set_terms(id, hash, specs)` | Stored client | Party (client) | `PAUSE_CREATE` | Blocked | Status `Created`, zero funded; zero or one spec hash per milestone |
| `deliver(id, index, hash)` | Stored freelancer | Party (freelancer) | `PAUSE_RELEASE` | Blocked | Status `Created`, `Funded`, or `Disputed`; milestone not released |
| `expire_unfunded(id)` | None | Anyone | `PAUSE_CANCEL` | Blocked | Status `Created`, zero released; funding deadline passed; partial funds refunded to recorded funders |
| `propose_settlement(id, proposer, freelancer_amount)` | `proposer` argument | Party | `PAUSE_CANCEL` | Blocked | Proposer is stored client or freelancer; status `Created`, `Funded`, or `Disputed`; amount within held balance |
| `accept_settlement(id, acceptor, freelancer_amount)` | `acceptor` argument | Party | `PAUSE_CANCEL` | Blocked | Acceptor is the other party; proposal exists and amount matches; status allows settlement |
| `withdraw_settlement(id, proposer)` | `proposer` argument | Party (original proposer) | None | Allowed while frozen | Proposal exists and its stored proposer equals the signer |
| `propose_amendment(id, proposer, amounts)` | `proposer` argument | Party | `PAUSE_CREATE` | Blocked | Non-stream escrow, status `Created` or `Funded`; proposer is a party; same milestone count, valid amounts, released amounts unchanged |
| `accept_amendment(id, acceptor, nonce)` | `acceptor` argument | Party | `PAUSE_CREATE` | Blocked | Acceptor is a party and not the proposer; nonce matches pending amendment; status allows amendment; revalidated against current state |
| `resign(id)` | Stored freelancer | Party (freelancer) | `PAUSE_CANCEL` | Blocked | Status `Created` or `Funded`; unreleased funds refunded to funders |
| `cancel_contract(id)` | Stored client | Party (client) | `PAUSE_CANCEL` | Blocked | Non-stream escrow, status `Created` or `Funded`; submitted milestone backing stays claimable |
| `issue_reputation(id, rating)` | Stored client | Party (client) | `PAUSE_REPUTATION` | Blocked | Status `Completed` or `Resolved`; not compacted; not already issued; rating 1..=5; released value at least `MIN_REPUTATION_VALUE` |
| `rate_client(id, rating)` | Stored freelancer | Party (freelancer) | `PAUSE_REPUTATION` | Blocked | Status `Completed` or `Resolved`; not already rated; rating 1..=5 |
| `void_rating(id, reason)` | Stored admin | Admin | `PAUSE_REPUTATION` | Allowed while frozen | Rating record exists and is not already voided; authentication precedes the pause check |
| `migrate_escrows(ids)` | None | Anyone | None | Allowed while frozen | At most 50 ids; already-migrated and unknown ids are skipped |
| `extend_escrow_ttl(id)` | None | Anyone | None | Allowed while frozen | Escrow or compact receipt must exist; caller pays rent extension |
| `extend_escrows_ttl(ids)` | None | Anyone | None | Allowed while frozen | At most `MAX_TTL_BATCH` (25) ids; unknown ids are skipped; returns the count extended |
| `extend_reputation_ttl(freelancer)` | None | Anyone | None | N/A | No authorization or existence precondition |
| `compact_escrow(id)` | None | Anyone | None | Blocked | Not already compacted; terminal status; `Cancelled` fully refunded or `Completed`/`Resolved` rated or past grace period |
| `grant_role(role, account)` | Stored admin | Admin | None | N/A | Admin initialized; membership must actually change, otherwise `RoleUnchanged` |
| `revoke_role(role, account)` | Stored admin | Admin | None | N/A | Admin initialized; membership must actually change, otherwise `RoleUnchanged` |
| `schedule_upgrade(hash)` | Stored admin | Admin | None | N/A | Admin initialized; stores hash with a 48-hour eta and restarts the delay on resubmission |
| `cancel_upgrade()` | Stored admin | Admin | None | N/A | A pending upgrade must exist |
| `execute_upgrade()` | Stored admin | Admin | None; allowed while paused | N/A | Pending upgrade exists and eta has elapsed; clears the pending record before installing WASM |

---

## 4. Authorization invariants

These properties are enforced by the code paths in the matrix above.

1. **No unauthenticated value transfer from a party account.** Every entrypoint that debits a client, freelancer, funder, or dispute opener calls `require_auth()` on that exact stored or validated address.
2. **Stored identity outranks argument identity.** Client-only and freelancer-only entrypoints authorize `contract.client` or `contract.freelancer` from storage. Argument-based entrypoints (`deposit_from`, `open_dispute`, settlement, amendment, panel voting) authorize the argument and then check it against the required role.
3. **Admin authority is explicit and narrow.** `require_admin` reads `DataKey::Admin`; `holds_role` gives the admin implicit `Pauser` and `Resolver` membership. A `Pauser` cannot resolve, and a `Resolver` cannot pause.
4. **Dispute resolution is conflict-free.** `resolve_dispute` rejects the client and freelancer even if they hold `Resolver`, and high-value escrows require a panel vote instead of a single resolver call.
5. **Pause is category-specific.** A `PAUSE_CREATE` flag blocks creation and setup but not cancellation; `PAUSE_REPUTATION` blocks ratings and voiding; `PAUSE_ALL` blocks all six categories. Recovery and governance actions are deliberately exempt.
6. **Freeze is escrow-local and party-facing.** Frozen escrows reject party mutations with `EscrowFrozen` but remain resolvable, emergency-exitable, migratable, TTL-extendable, and voidable by admin governance.
7. **Permissionless entrypoints are predicate-bound.** `expire_unfunded`, `migrate_escrows`, `extend_escrow_ttl`, `extend_escrows_ttl`, `extend_reputation_ttl`, and `compact_escrow` require no signature, but each is limited by status, deadline, batch size, existence, or receipt rules.
8. **No hidden timelock.** Admin rotation is two-step but does not enforce a delay in this contract. The 48-hour delay applies only to `execute_upgrade`, and the 30-day delay applies only to `emergency_withdraw`.

---

## 5. Test cross-checks

The MockAuth tests in `contracts/escrow/src/test.rs` verify the signer column rather than merely relying on comments.

| Test | What it verifies |
|------|------------------|
| `client_cannot_deliver_and_freelancer_cannot_set_terms` | `deliver` requires freelancer auth; `set_terms` requires client auth. Authorizing the wrong party fails for both. |
| `bonus_requires_client_auth` | `pay_bonus` requires client auth; freelancer auth alone fails and no bonus is recorded. |
| `client_cannot_set_payout_address` | `set_payout_address` requires freelancer auth; client auth alone fails. |
| `submit_milestone_failure_paths` and `claim_milestone_failure_paths` | Client auth cannot substitute for freelancer auth, and `PAUSE_RELEASE` blocks both. |
| `test_rate_client_authorization` | `rate_client` accepts only freelancer auth; client and third-party auth fail. |
| `client_calling_resign_rejected` | `resign` requires freelancer auth; client auth fails. |
| `stream_auth_schedule_and_milestone_api_guards` | `withdraw_vested` requires freelancer auth; client auth fails. |
| `pauser_can_pause_but_cannot_resolve_disputes` and `resolver_can_resolve_but_cannot_pause` | `Pauser` and `Resolver` are distinct; admin holds both implicitly. |
| `grant_and_revoke_role_unchanged_and_admin_only` | Role grants and revocations require admin auth. |
| `propose_rejects_self_and_non_admin` and `accept_by_non_nominee_fails` | Admin rotation signers are the stored admin for propose and the pending nominee for accept. |
| `admin_transfer_works_while_paused` | Admin rotation is exempt from the pause gate. |
| `test_non_admin_freeze_rejected` and `test_freeze_error_conditions` | Freeze and unfreeze require the stored admin. |
| `test_per_escrow_freeze_isolation_and_unfreeze` | Freeze blocks client and freelancer mutations but is isolated per escrow and reversible by admin. |
| `test_resolve_dispute_allowed_on_frozen_escrow` | Resolution remains available on a frozen escrow. |
| `each_pause_flag_gates_exactly_its_entry_points` | Each pause bit gates only its own category. |
| `inflow_pause_still_lets_clients_cancel_and_refund` | Create and deposit can be paused while cancellation remains available. |
| `vote_change_replaces_vote_and_requires_membership` | Non-members and parties cannot vote; only current panel votes count. |
| `test_partial_deposit_refund_third_party` | `expire_unfunded` is permissionless and refunds funders rather than the caller. |

---

## 6. Related documents

| Topic | Document |
|-------|----------|
| Role and storage layout | `docs/escrow/access-control.md`, `docs/escrow/authorization-storage.md` |
| Full ABI surface | `docs/escrow/abi-reference.md` |
| Dispute resolution flow | `docs/escrow/dispute-resolution.md` |
| Emergency exit rules | `docs/escrow/emergency-controls.md` |
| Protocol fee rules | `docs/escrow/protocol-fees.md` |
| Storage and TTL model | `docs/escrow/state-persistence.md`, `docs/escrow/storage-ttl.md` |
| Security analysis | `docs/escrow/SECURITY.md` |
