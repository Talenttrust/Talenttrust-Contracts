# Escrow Threat Model

This note documents the trust assumptions, attacker capabilities, and mitigations for the escrow contract in `contracts/escrow`. It reflects the **live** contract binary: the `#[contractimpl]` blocks in `contracts/escrow/src/lib.rs`, `contracts/escrow/src/create_contract.rs`, and `contracts/escrow/src/governance.rs`. Files such as `contracts/escrow/src/release.rs` and `contracts/escrow/src/refund_impl.rs` are present in the source tree but are not declared as modules in `lib.rs` and are therefore not compiled into the current binary.

## Trust Assumptions

- **Soroban authentication primitives are correct.** `Address.require_auth()` is the only way the escrow contract can prove a caller controls an address. The contract does not maintain private keys or off-chain identity.
- **The stored admin is trusted.** A single admin controls pause, emergency, protocol-fee configuration, settlement-token binding, and governed parameters. There is no on-chain multi-sig or timelock for day-to-day admin actions.
- **The settlement token custody model is outside this contract.** The escrow records accounting and instructs the Stellar Asset Contract (SAC) to transfer tokens. The token contract is trusted for actual custody, minting, and transfer semantics.
- **Off-chain clients validate returned state.** The contract emits events and returns `ContractStatus`, `MilestoneApprovals`, `FinalizationRecord`, and balances. UIs should treat anything shown from storage as untrusted until it matches an on-chain query.
- **Ledger time and sequence are authoritative.** Deadline, TTL, and timelock computations use `env.ledger().timestamp()` and `env.ledger().sequence()` and are not manipulable by contract callers.

## Attacker Capabilities and Attack Surface

An external attacker may attempt to:

| Capability | Surface | Impact if unmitigated |
|---|---|---|
| Spoof an `Address` argument | Any `pub fn` taking a `caller`/`client`/`arbiter` address | Unauthorized state changes, fund release, or refunds |
| Replay or forge milestone approvals | `MilestoneApprovals` temporary storage | Milestone released without real consent |
| Double release or refund | `milestone.released` / `milestone.refunded` flags | Same milestone paid twice or refunded twice |
| Over-fund or over-refund | `funded_amount` / `released_amount` / `refunded_amount` accounting | Balance invariant broken or funds drained |
| Block operations | `Paused` / `Emergency` flags | Denial of service if admin key compromised |
| Manipulate reputation | `PendingReputationCredits` and `Reputation` storage | Inflated freelancer reputation |
| Abuse TTL expiry | Temporary storage (`MilestoneApprovals`, `PendingClientMigration`) | Stale approvals/migrations expired or kept alive by reads |
| Resolve disputes unfairly | `resolve_dispute` accounting updates | Arbiter can reallocate accounting, but **cannot move SAC tokens directly** |

## Mitigations

### Auth gating

Every mutating entrypoint that changes escrow state requires `require_auth()` from an authorized address. See the full cross-reference below.

### State-machine guards

- `require_not_paused` (`contracts/escrow/src/finalize.rs:48`) blocks mutating lifecycle calls when `Paused` or `Emergency` is set.
- `require_not_finalized` (`contracts/escrow/src/finalize.rs:42`) prevents any further contract-specific mutation after `finalize_contract` writes a `FinalizationRecord`.
- `require_finalizer_role` (`contracts/escrow/src/finalize.rs:67`) restricts finalization to the stored client, freelancer, or assigned arbiter.
- Terminal-state checks reject `Cancelled` / `Refunded` contracts from new deposits, releases, or refunds.

### Amount and accounting validation

- `create_contract` enforces distinct participants, arbiter validity, non-empty milestones, `MAX_MILESTONES` (10), per-milestone bounds, and a total cap via `amount_validation::validate_milestone_amounts` (`contracts/escrow/src/create_contract.rs:41–102`).
- `deposit_funds` validates positivity, state, and `caller == client` before the SAC transfer and applies the deposit with `caller.require_auth()` (`contracts/escrow/src/deposit.rs:19–125`).
- `release_milestone` verifies `available_balance >= gross_amount`, recomputes `available_balance` after accumulated fees, and enforces `released_amount + refunded_amount + accumulated_fees <= funded_amount` (`contracts/escrow/src/lib.rs:690–874`).
- `refund_unreleased_milestones` validates each milestone is not released/refunded, is overdue if a deadline exists, and that the contract has sufficient balance (`contracts/escrow/src/lib.rs:1018–1148`).
- Dispute payout arithmetic is isolated in `dispute::resolution_payouts`, which checks non-negative splits, overflow, and exact conservation of the available balance (`contracts/escrow/src/dispute.rs:30–69`).

### Approval lifecycle

- `approve_milestone_release` records approvals in temporary storage with a TTL (`PENDING_APPROVAL_TTL_LEDGERS`).
- `release_milestone` requires valid, non-expired approvals as determined by `approvals::check_approvals` (`contracts/escrow/src/approvals.rs:180–212`), which treats missing/expired records as insufficient.
- `approvals::clear_approvals` removes the record after a successful release to prevent reuse (`contracts/escrow/src/approvals.rs:222–225`).

## Auth Check Cross-Reference

| Entrypoint | Required Authorizer | Source | Notes |
|---|---|---|---|
| `initialize(admin)` | `admin` | `contracts/escrow/src/lib.rs:376` | Single-use; sets `Initialized` and `Admin`. |
| `bind_settlement_token(admin, token)` | `admin == stored_admin` | `contracts/escrow/src/lib.rs:267` | Write-once settlement token binding. |
| `set_settlement_token(...)` | (deprecated) | `contracts/escrow/src/lib.rs:330` | Delegates to `bind_settlement_token`. |
| `create_contract(..., client, ...)` | `client` | `contracts/escrow/src/create_contract.rs:54` | Also enforces distinct client/freelancer/arbiter. |
| `deposit_funds(..., caller, amount)` | `caller == contract.client` | `contracts/escrow/src/deposit.rs:35` then `caller.require_auth()` at `125` | Preflight validation before SAC transfer. |
| `approve_milestone_release(..., caller, ...)` | **None** | `contracts/escrow/src/lib.rs:606` → `approvals.rs:46` | No `require_auth()` on `caller`; approvals can be recorded for an arbitrary address. |
| `release_milestone(..., caller, ...)` | `caller` + role check | `contracts/escrow/src/lib.rs:698` and `722–743` | Mode-specific `ReleaseAuthorization` check after auth. |
| `refund_unreleased_milestones(...)` | `contract.client` | `contracts/escrow/src/lib.rs:1059` | Refunds only unreleased, non-refunded, overdue-if-deadline milestones. |
| `cancel_contract(..., client)` | `client == contract.client` | `contracts/escrow/src/lib.rs:1604` then `client.require_auth()` at `1620` | Requires no released funds. |
| `issue_reputation(..., caller, ...)` | `caller == contract.client` | `contracts/escrow/src/lib.rs:1696` then `caller.require_auth()` at `1723` | Requires `Completed` status and unused reputation. |
| `finalize_contract(..., finalizer)` | `finalizer` + role check | `contracts/escrow/src/finalize.rs:142` and `67` | Allowed only from `Completed` or `Disputed`. |
| `propose_client_migration(..., current_client, ...)` | `current_client == contract.client` | `contracts/escrow/src/migration.rs:55` | Stored in temporary storage with TTL. |
| `accept_client_migration(..., new_client)` | `new_client == pending.proposed_client` | `contracts/escrow/src/migration.rs:99` | Replaces the stored client. |
| `cancel_client_migration(..., current_client)` | `current_client == contract.client` | `contracts/escrow/src/migration.rs:133` | Removes a pending migration. |
| `raise_dispute(..., caller)` | `caller` and `caller == client or freelancer` | `contracts/escrow/src/lib.rs:2189` and `2201` | Requires an assigned arbiter and `Funded`/`PartiallyFunded` state. |
| `resolve_dispute(..., arbiter, ...)` | `arbiter == contract.arbiter` | `contracts/escrow/src/lib.rs:2273` and `2290` | Updates accounting; does **not** move SAC tokens. |
| `pause()` | stored `admin` | `contracts/escrow/src/lib.rs:1431` | Sets `Paused`. |
| `unpause()` | stored `admin` | `contracts/escrow/src/lib.rs:1457` | Blocked while `Emergency` is active. |
| `activate_emergency_pause()` | stored `admin` | `contracts/escrow/src/lib.rs:1492` | Sets both `Emergency` and `Paused`. |
| `resolve_emergency()` | stored `admin` | `contracts/escrow/src/lib.rs:1545` | Clears both flags. |
| `set_protocol_fee_bps(new_bps)` | stored `admin` | `contracts/escrow/src/governance.rs:39` | Capped at `10_000` bps. |
| `set_governed_params(admin, ...)` | `admin == stored_admin` | `contracts/escrow/src/governance.rs:224` | Sets protocol fee and escrow cap. |
| `withdraw_protocol_fees(amount, to)` | stored `admin` | `contracts/escrow/src/lib.rs:2030` | Transfers only accumulated fees. |

## Residual Risks and Known Gaps

- **`approve_milestone_release` does not authenticate `caller`.** Because neither `lib.rs` nor `approvals.rs` calls `caller.require_auth()`, any address can record an approval for another address. This is a live auth gap.
- **`resolve_dispute` updates accounting without SAC transfers.** It modifies `released_amount` and `refunded_amount` but does not transfer tokens to the client or freelancer; a separate off-chain or integration step must settle the actual asset movement, and accounting can diverge from token balance if not reconciled.
- **One admin, no timelock on operational controls.** `pause`, `unpause`, `emergency`, `withdraw_protocol_fees`, `set_protocol_fee_bps`, and `bind_settlement_token` all require only the stored admin. Two-step admin transfer helpers exist (`propose_governance_admin_impl` / `accept_governance_admin_impl`), but they are `pub(crate)` in `governance.rs` and have no public wrapper entrypoint.
- **Token custody is external.** The escrow does not custody tokens natively; it relies on the bound SAC. Any bug or misconfiguration in the token contract or the bound address is outside the scope of this contract.
