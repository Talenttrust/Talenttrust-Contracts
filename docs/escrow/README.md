# Escrow Integration Guide

This guide documents the entrypoints currently implemented by the escrow
contract. Planned features are listed separately and linked to their tracking
issues so integrators can distinguish live API from roadmap.

## Module Map

- `contracts/escrow/src/lib.rs`: contract type, shared API surface, reads, controls, cancellation, reputation, and module wiring.
- `contracts/escrow/src/create_contract.rs`: `create_contract` lifecycle entrypoint.
- `contracts/escrow/src/deposit.rs`: `deposit_funds` lifecycle entrypoint (SAC-aware; pulls tokens from client to escrow).
- `contracts/escrow/src/release_milestone.rs`: inlined in `lib.rs`; transfers tokens from escrow to freelancer net of protocol fee.
- `contracts/escrow/src/refund.rs`: `refund_unreleased_milestones` lifecycle entrypoint.
- `contracts/escrow/src/test/sac_custody.rs`: SAC custody tests for `bind_settlement_token`, `deposit_funds` (SAC path), and `release_milestone` (SAC path).
- `contracts/escrow/src/test/cancel_contract.rs`: cancellation tests, including SAC client-refund and shared-escrow balance assertions.

## Implemented API Surface

Lifecycle and reputation:

- `create_contract(client, freelancer, milestone_amounts, deposit_mode) -> u32`
- `deposit_funds(contract_id, amount) -> bool`
- `submit_work_evidence(contract_id, caller, milestone_index, evidence) -> bool`
- `release_milestone(contract_id, milestone_index) -> bool`
- `issue_reputation(contract_id, caller, rating, comment) -> bool`
- `cancel_contract(contract_id, caller) -> bool`
- `finalize_contract(contract_id, finalizer) -> bool`

SAC settlement-token binding:

- `bind_settlement_token(token) -> bool` *(admin-only, single-use; binds the Stellar Asset Contract used for custody)*
- `get_settlement_token() -> Option<Address>`

Read-only queries:

- `get_contract(contract_id) -> EscrowContractData`
- `get_milestones(contract_id) -> Vec<Milestone>`
- `get_refundable_balance(contract_id) -> i128`
- `get_milestone_approvals(contract_id, milestone_index) -> Option<MilestoneApprovals>`
- `get_finalization_record(contract_id) -> Option<FinalizationRecord>`
- `get_reputation(freelancer) -> Option<ReputationRecord>`
- `get_average_rating(freelancer) -> Option<i128>` — scaled average (see [Average Rating](#average-rating))
- `get_pending_reputation_credits(freelancer) -> i128`
- `get_admin() -> Option<Address>`
- `get_settlement_token() -> Option<Address>`
- `is_paused() -> bool`
- `is_emergency() -> bool`
- `get_mainnet_readiness_info() -> MainnetReadinessInfo`
- `get_protocol_fee_bps() -> u32`
- `get_accumulated_protocol_fees() -> i128`
- `get_bounds() -> ContractBounds` *(returns the compile-time protocol bounds: max milestones, max single milestone amount, max total escrow amount, max fee bps; see [`ContractBounds`](../../contracts/escrow/src/types.rs))*

### Read-only getter semantics

The read getters below are stable, side-effect-free paths that indexers and
off-chain callers rely on. They share three properties:

1. **Not-found**: every getter that takes a `contract_id` panics the contract
   with `ContractNotFound` when `contract_id` was never allocated. The
   Soroban-generated `try_*` wrappers surface this as
   `Err(Ok(ContractNotFound))` for off-chain callers and do not mutate any
   other persistent state.
2. **Pure read**: invoking any of these getters on a valid `contract_id` does
   not mutate balances, status, milestones, or per-milestone flags.
   Accounting-only fields (`funded_amount`, `released_amount`,
   `refunded_amount`) and per-milestone `released`/`refunded` flags are
   bitwise-stable across arbitrary numbers of repeated calls.
3. **TTL on read (persistent only)**: on a successful read the contract
   extends the persistent TTL of the entry being read from (`Contract(id)`,
   `(Contract(id), "milestones")`) to `PERSISTENT_TTL_LEDGERS` (30 days).
   This keeps idle but live contracts in storage without rebuilding them. The
   `get_milestone_approvals` getter reads from temporary storage and is
   therefore exempt from this rule; it is governed by
   `PENDING_APPROVAL_TTL_LEDGERS` and the host's auto-eviction.

Per-getter details:

- `get_contract(contract_id)` returns the full `EscrowContractData`
  (participants, arbiter, status, funded/released/refunded amounts,
  release_authorization). Reads persist the contract entry's TTL. Panics
  `ContractNotFound` for an unknown id.
- `get_milestones(contract_id)` returns the milestones vector in creation
  order. Reads persist the milestones entry's TTL. Panics `ContractNotFound`
  for an unknown id.
- `get_refundable_balance(contract_id)` returns
  `funded_amount - released_amount - refunded_amount`. The result must be
  non-negative by construction; panic-on-overflow is enforced on
  contributing arithmetic at every mutating entrypoint. Reads persist the
  contract entry's TTL. Panics `ContractNotFound` for an unknown id.
- `get_milestone_approvals(contract_id, milestone_index)` returns `Some`
  only if a non-expired approval record for that milestone exists in
  temporary storage. Returns `None` when no approval has been recorded or
  when the contract id is unknown. Does not extend persistent TTL because
  approvals live in temporary storage bounded by
  `PENDING_APPROVAL_TTL_LEDGERS`.

These properties are locked in by tests under
`contracts/escrow/src/test/persistence.rs` (issue #475).

### ContractBounds vs ContractSummary

`get_bounds()` and `get_contract_summary()` both return structured read results
but serve different purposes and must not be conflated:

| Type | Returned by | Contains | Changes with |
| --- | --- | --- | --- |
| `ContractBounds` | `get_bounds()` | Protocol-wide compile-time limits (max milestones, max amount, max fee bps) | Binary update |
| `ContractSummary` | `get_contract_summary()` | Per-contract state snapshot (participants, status, amounts, milestones) | Each mutating call |

`ContractBounds` has no participant addresses, no accounting fields, and no
milestones vector. Its `schema_version` tracks the limits ABI only.
`ContractSummary` is the per-contract snapshot used by `get_contract_summary`
and embedded in `FinalizationRecord`; its schema version tracks per-contract
data.

Indexers discovering limits should call `get_bounds()`. Indexers snapshotting
contract state should call `get_contract_summary()`.

Operational controls:

- `initialize(admin) -> bool`
- `pause() -> bool`
- `unpause() -> bool`
- `activate_emergency_pause() -> bool`
- `resolve_emergency() -> bool`

Governance admin transfer (two-step):

- `propose_governance_admin(proposed) -> bool`
- `accept_governance_admin() -> bool`
- `cancel_governance_admin_proposal() -> bool`
- `get_governance_admin() -> Option<Address>`
- `get_pending_governance_admin() -> Option<Address>`

## Canonical Happy Path

### 1. Initialize Operational Admin and Bind Settlement Token

```rust
escrow.initialize(&admin);
let sac = /* deployed Stellar Asset Contract address */;
escrow.bind_settlement_token(&sac);
```

`initialize` is single-use, requires `admin.require_auth()`, and stores the
admin used by pause, emergency, fee, and token-binding controls.
`bind_settlement_token` is also single-use (a second call panics with
`SettlementTokenAlreadyBound`) and emits a `(settl_tok, "bound")` audit event.

### 2. Create Contract

```rust
let contract_id = escrow.create_contract(
    &client_addr,
    &freelancer_addr,
    &None,
    &vec![&env, 500_0000000_i128, 500_0000000_i128],
    &ReleaseAuthorization::ClientOnly,
);
```

Creation requires `client.require_auth()`, rejects identical client/freelancer
addresses, rejects empty or non-positive milestones, caps milestone count at
`MAX_MILESTONES` (rejecting with `TooManyMilestones`), and caps total escrow value
against governed `max_escrow_total_stroops` (rejecting with `TotalCapExceeded`).

### 3. Deposit Funds (SAC debit)

```rust
escrow.deposit_funds(&contract_id, &client_addr, &1000_0000000_i128);
```

`deposit_funds` now performs a real on-chain transfer: the escrow contract
calls `token::Client::transfer(caller, escrow, amount)` against the bound
settlement token BEFORE updating `funded_amount`. If the SAC transfer fails
the contract accounting is unchanged, so a partial-deposit run can never
leave the accounting counter ahead of the actual custody balance.

`ExactTotal` contracts require one exact deposit equal to the milestone total.
`Incremental` contracts allow partial deposits until the milestone total is
reached. Deposits that exceed the required total fail closed with
`InvalidDepositAmount`.

### 4. Submit Work Evidence (optional, before release)

```rust
escrow.submit_work_evidence(
    &contract_id,
    &freelancer_addr,
    &0,                                   // milestone_index
    &String::from_str(&env, "ipfs://QmExampleCid"),
);
```

The freelancer may attach a deliverable reference (IPFS CID, URL hash, or any
string up to 256 bytes) to an unreleased milestone before the client approves
it. Evidence can be overwritten any number of times prior to release; once the
milestone is released or refunded the field is immutable.

Guards applied:
- `ContractPaused` / `EmergencyActive` — pause/emergency gate.
- `ContractNotFound` — unknown `contract_id`.
- `AlreadyFinalized` — contract has been finalized.
- `UnauthorizedRole` — caller is not the stored freelancer.
- `InvalidState` — contract is not in `Funded` status.
- `IndexOutOfBounds` — `milestone_index` exceeds the milestone count.
- `MilestoneAlreadyReleased` — milestone has already been released.
- `AlreadyRefunded` — milestone has already been refunded.
- `EvidenceTooLong` — evidence string exceeds 256 bytes.

Emits `("evidence", contract_id)` with `(milestone_index, freelancer, timestamp)`.

### 5. Release Milestones

#### Single Milestone Release
```rust
escrow.release_milestone(&contract_id, &client_addr, &0);
```

`release_milestone` performs a real on-chain payout:

1. All existing pre-conditions (pause gate, auth, approval, role /
   `ReleaseAuthorization`, milestone state, balance) hold.
2. The escrow reads the bound settlement token; if no token has been bound it
   panics with `SettlementTokenNotConfigured`.
3. The escrow reads the protocol fee (set via `set_protocol_fee_bps`, capped at
   `10_000` basis points / 100%); the
   payout to the freelancer is `milestone.amount - fee`, with `fee` retained
   inside the contract via `DataKey::AccumulatedProtocolFees`.
4. `token::Client::transfer(escrow, freelancer, payout)` is invoked BEFORE
   the milestone is marked released and the contract status is updated — so
   a token-transfer failure leaves the contract untouched.

When the final milestone is released, status becomes `Completed` and one
pending reputation credit is added for the freelancer.

`PendingReputationCredits` is a non-negative counter that tracks completed
contracts awaiting client-issued reputation for a freelancer. `issue_reputation`
consumes one pending credit and records the rating.

### Release Authorization Mode / Caller Matrix

The `ReleaseAuthorization` field set at contract creation controls which
parties may approve a milestone and which may trigger the on-chain release.
The table below is the definitive cross-reference for all four modes.

| Mode | Allowed Approvers | Approval Logic | Allowed Release Callers | Required Contract-Creation Arg |
|---|---|---|---|---|
| `ClientOnly` (0) | Client | `client_approved == true` | Client | Arbiter optional |
| `ClientAndArbiter` (1) | Client **or** Arbiter | `client_approved \|\| arbiter_approved` | Client **or** Arbiter | Arbiter **required** |
| `ArbiterOnly` (2) | Arbiter | `arbiter_approved == true` | Arbiter | Arbiter **required** |
| `MultiSig` (3) | Client **and** Freelancer | `client_approved && freelancer_approved` | Client **or** Freelancer | Arbiter optional |

**Key behavioral rules enforced by `release_milestone` and `approve_milestone_release`:**

1. **Caller role check** — `release_milestone` first authenticates the caller
   via `caller.require_auth()`, then checks the caller's role against the
   mode. Callers whose role is not listed in "Allowed Release Callers" receive
   `UnauthorizedRole` before any approval logic runs.

2. **Approval presence** — after the role check passes, `check_approvals`
   verifies that the required approvals are present in temporary storage. If
   the approval record is absent or has expired, the call fails with
   `InsufficientApprovals`. No partial state change occurs.

3. **Approval scope** — `approve_milestone_release` enforces the same
   role rules: a party whose role is not listed in "Allowed Approvers" is
   rejected with `UnauthorizedRole` and no approval is recorded.

4. **MultiSig AND vs OR** — `MultiSig` is the only mode with AND semantics:
   *both* client and freelancer must have approved before *either* can
   trigger the release. Attempting a release after only one of the two
   approves yields `InsufficientApprovals`.

5. **Approval expiry** — approvals are stored in Soroban temporary storage
   with a TTL of `PENDING_APPROVAL_TTL_LEDGERS` (~7 days). Expired approvals
   are treated as absent; `check_approvals` cannot distinguish expired from
   never-recorded and returns `InsufficientApprovals` in both cases.

6. **Approval clearing** — approvals are removed from temporary storage
   immediately after a successful release. They cannot be reused for a
   different milestone or a second attempt on the same milestone.

7. **InvalidState guard** — `release_milestone` requires the contract to be
   in the active funded state. Contracts in `Created`, `Completed`,
   `Cancelled`, or `Refunded` status are rejected with `InvalidState` before
   any role or approval logic runs.

**Error code quick-reference:**

| Condition | Error |
|---|---|
| Caller's role is not in "Allowed Release Callers" | `UnauthorizedRole` |
| Required approvals missing or expired | `InsufficientApprovals` |
| Contract not in active funded state | `InvalidState` |
| Approval attempted by unauthorized party | `UnauthorizedRole` |
| Same party approves twice | `AlreadyApproved` |
| Milestone already released | `MilestoneAlreadyReleased` |
| Milestone already refunded | `AlreadyRefunded` |

Test coverage for every cell in the matrix above lives in
`contracts/escrow/src/test/release_authorization.rs` (issue #710).

### 5. Issue Reputation

```rust
escrow.issue_reputation(&contract_id, &client_addr, &freelancer_addr, &5_i128);
```

Reputation requires `caller.require_auth()`, the caller must be the stored
client, the freelancer argument must match the contract freelancer, the contract
must be `Completed`, rating must be `1..=5`, and each contract can issue
reputation once.

## Custody Lifecycle (SAC token integration)

The escrow holds a real Stellar Asset Contract (SAC) balance for the lifetime
of each contract. The flow is fully on-chain and atomic — every state change
to the contract's accounting counters is paired with the matching SAC
`transfer` call. There is no off-chain reconciliation step.

| Step | Entrypoint | SAC operation | State change |
|---|---|---|---|
| Token binding (admin, single-use) | `bind_settlement_token(sac)` | `—` | `DataKey::SettlementToken = sac` |
| Funding | `deposit_funds(id, client, amount)` | `transfer(client, escrow, amount)` | `contract.funded_amount += amount` |
| Release | `release_milestone(id, caller, idx)` | `transfer(escrow, freelancer, milestone.amount - fee)` | `milestone.released = true`, `contract.released_amount += milestone.amount`, `DataKey::AccumulatedProtocolFees += fee` |
| Refund | `refund_unreleased_milestones(id, indices)` | `transfer(escrow, client, sum)` | `milestone.refunded = true`, `contract.refunded_amount += sum` |

The pause/emergency gate, fail-closed validation, and TTL bumps from the
existing lifecycle are preserved unchanged on each path.

### Failure semantics

| Failure | Behaviour |
|---|---|
| `bind_settlement_token` called twice | `SettlementTokenAlreadyBound` panic; existing token retained |
| `deposit_funds` with no token bound | `SettlementTokenNotConfigured` panic; funded_amount unchanged |
| `deposit_funds` with insufficient SAC balance | SAC transfer fails; `TokenTransferFailed` panic via Soroban's contract-error return; funded_amount unchanged |
| `release_milestone` with no token bound | `SettlementTokenNotConfigured` panic; milestone state unchanged |
| `release_milestone` with insufficient SAC balance | SAC transfer fails; milestone state unchanged |

## Cancellation

```rust
escrow.cancel_contract(&contract_id, &caller);
```

Cancellation requires `caller.require_auth()`. The caller must be the stored
client or freelancer. It is blocked after `Completed` and blocked if the
contract is already `Cancelled`.

## Finalization

```rust
escrow.finalize_contract(&contract_id, &finalizer);
```

Finalization requires `finalizer.require_auth()`. The finalizer must be the
stored client, freelancer, or assigned arbiter. It is allowed only while the
contract status is `Completed` or `Disputed`.

The contract writes one immutable `FinalizationRecord` containing the finalizer,
ledger timestamp, and a `ContractSummary` snapshot. After the record exists,
contract-specific mutating calls reject with `AlreadyFinalized`.

## Client Migration

```rust
escrow.propose_client_migration(&contract_id, &current_client, &new_client);
escrow.accept_client_migration(&contract_id, &new_client);
```

Client migration is a two-step, time-boxed handover of the `client` role on an
existing contract:

1. `propose_client_migration` requires `current_client.require_auth()`, the
   caller must be the stored client, and the proposed address must not be the
   current client or the freelancer. It stores a `PendingClientMigration` record
   in temporary storage.
2. `accept_client_migration` requires `new_client.require_auth()` and the
   acceptor must match the proposed address; on success it updates
   `contract.client` and clears the pending record.

### Migration window (TTL)

The pending record is stored with a fixed time-to-live of
`PENDING_MIGRATION_TTL_LEDGERS = LEDGERS_PER_DAY * 21` ledgers — roughly **21
days** at ~5s/ledger. The record's `expires_at_ledger` is set to
`requested_at_ledger + PENDING_MIGRATION_TTL_LEDGERS`.

- **Within the window** (current ledger `< expires_at_ledger`): acceptance by
  the proposed address succeeds and transfers client rights.
- **After the window**: Soroban auto-evicts the temporary entry, so
  `read_if_live` returns `None`. `has_pending_client_migration` then reports
  `false` and `accept_client_migration` fails with `InvalidState`.

Security assumption: an expired proposal cannot transfer client rights. Once the
TTL lapses the stale proposal is unrecoverable; the current client must submit a
fresh `propose_client_migration` call to start a new window.
## Two-Step Governance Admin Transfer

The admin transfer uses a propose-accept (two-step) pattern:

```rust
// Step 1: current admin proposes next admin
escrow.propose_governance_admin(&next_admin);

// Step 2: next admin accepts (requires next_admin.require_auth())
escrow.accept_governance_admin();
```

### Rules
- **Self-proposal is rejected**: proposing the current admin as the new admin
  panics with `CannotProposeSelf`.
- **Re-proposing overwrites**: calling `propose_governance_admin` while a
  pending proposal exists silently replaces it (no explicit cancellation
  required).
- **Cancellation**: `cancel_governance_admin_proposal` is admin-gated (only
  the current admin may cancel). It clears the pending admin and emits a
  `("admin", "cancelled")` event.
- **No stale acceptance**: accepting after cancellation panics with
  `InvalidState` because the pending proposal has been removed.
- All operations require the contract to be initialized.

### Events
| Topic | Data | Trigger |
|---|---|---|
| `("admin", "proposed")` | `(admin, proposed, timestamp)` | `propose_governance_admin` |
| `("admin", "accepted")` | `(old_admin, new_admin, timestamp)` | `accept_governance_admin` |
| `("admin", "cancelled")` | `(admin, cancelled_proposal, timestamp)` | `cancel_governance_admin_proposal` |

## Pause and Emergency Controls

`pause`, `unpause`, `activate_emergency_pause`, and `resolve_emergency` require
the stored admin's authorization. While paused or in emergency, mutating
lifecycle calls fail with `ContractPaused`; read-only queries remain available.
`unpause` fails while emergency mode is active.

## Events

Implemented events:

- `("evidence", contract_id)` on `submit_work_evidence` (payload: milestone_index, freelancer, timestamp)
- `("init", "admin_set")` on `initialize`
- `("settl_tok", "bound")` on `bind_settlement_token`
- `("paused", timestamp)` on `pause`
- `("unpaused", timestamp)` on `unpause`
- `("emergency", "activated")` and `("emergency", "resolved")`
- `("audit", contract_id)` for lifecycle state transitions
- `("created", contract_id)` on contract creation
- `("deposited", contract_id)` on deposit (with payload `(caller, amount, funded_amount, total, settlement_token)`)
- `("released", contract_id, milestone_index)` on release (with payload `(freelancer, payout, fee, settlement_token)`)
- `("rep_issd", contract_id)` on reputation issuance
- `("cancelled", contract_id)` on cancellation
- `("finalized", contract_id)` on finalization

The `("deposited", contract_id)` event is emitted on every successful
`deposit_funds` call (previously only status-changing deposits surfaced an
event). It includes the settlement-token address so off-chain indexers can
correlate the audit event with the SAC's own `transfer` event.

The `("released", contract_id, milestone_index)` event's payload now also
includes the gross payout, retained fee, and settlement-token address so
indexers can reconcile the escrow's accounting against the SAC's
`transfer` events without re-deriving fee math.

## Implemented Security Assumptions

- Creation and reputation issue require explicit address authentication.
- Pause and emergency controls are admin-authenticated.
- The settlement token is admin-bound once via `bind_settlement_token`; a
  second call panics with `SettlementTokenAlreadyBound` and is audited via
  the `("settl_tok", "bound")` event.
- Deposits pull real SAC tokens from the client via `token::Client::transfer`
  BEFORE updating `funded_amount`, so a failed transfer leaves accounting
  untouched.
- Deposits cannot exceed the exact milestone total; over-funding is detected
  via `checked_add` and panics with `InvalidDepositAmount`.
- Releases pay the freelancer (less protocol fee) via
  `token::Client::transfer` BEFORE updating milestone/contract state, so a
  failed payout leaves state untouched.
- Releases fail on duplicate milestone release, invalid milestone id, missing
  contract, paused state, and insufficient funded balance.
- Arithmetic for escrow totals, deposits, and releases uses checked helpers and
  panics with `PotentialOverflow` on overflow.
- Accounting is checked after balance-changing operations.
- The contract stores accounting state only; token custody and token transfers
  are not implemented in `lib.rs` and must be handled by an audited integration.
- Storage uses persistent keys for live contract state. Pending client
  migrations are written to temporary storage with a 21-day TTL (see
  [Client Migration](#client-migration)); the pending-approval TTL constant
  exists for a planned approval flow with no current writing entrypoint.
  contract, paused state, missing settlement token, and insufficient funded
  balance.
- Arithmetic for escrow totals, deposits, and releases uses checked helpers
  and panics with `PotentialOverflow` or `InvalidDepositAmount` on overflow.
- Accounting is checked after balance-changing operations; SAC balance and
  accounting counter cannot diverge through any tested path.

## Refund Accounting Invariant

`get_refundable_balance` is the security-critical query that answers "how much
of the escrowed funds can still be returned or released?". Its definition is:

```
refundable_balance = funded_amount - released_amount - refunded_amount
```

This identity must hold at every point in the contract's lifecycle. Violating
it would mean the contract could either over-pay (release or refund more than
was funded) or leave funds permanently locked.

### Properties the invariant guarantees

| Property | Explanation |
|---|---|
| Non-negative | `refundable_balance >= 0` at all times — the contract never becomes insolvent. |
| Zero iff all milestones terminal | Balance reaches `0` only when every milestone is in `released` or `refunded` state. |
| Additive decomposition | `funded_amount == released_amount + refunded_amount + refundable_balance` always. |
| Status consistency | When balance is `0` and any milestone was released, status is `Completed`; when all were refunded, status is `Refunded`. |

### When balance changes

- `deposit_funds` increases `funded_amount`, so balance increases.
- `release_milestone` increases `released_amount` by the milestone amount, so balance decreases.
- `refund_unreleased_milestones` increases `refunded_amount` by the sum of the refunded milestones, so balance decreases.
- No other entrypoint mutates these three fields.

### Test coverage

`contracts/escrow/src/test/refund.rs` contains a dedicated suite of accounting
invariant tests (`assert_balance_invariant`) that verify this property after
every operation order:

- balance equals `funded_amount` before any release or refund
- release-then-refund sequence
- refund-then-release sequence
- all milestones released → balance zero, status `Completed`
- all milestones refunded → balance zero, status `Refunded`
- interleaved alternating operations at every step
- cross-check that `get_refundable_balance` matches `get_contract` fields exactly
- partial deposit with partial refund never goes negative

## Dispute Resolution Payout Matrix

When a dispute is raised on a funded or partially funded contract, the assigned arbiter
chooses a resolution. The contract enforces conservation of the available balance:

```
available = funded_amount - released_amount - refunded_amount
client_payout + freelancer_payout == available
```

### Resolution Variants

| Variant | Client Payout | Freelancer Payout | Notes |
|---|---|---|---|
| `FullRefund` | `available` | `0` | All remaining funds to client. Status becomes `Refunded`. |
| `FullPayout` | `0` | `available` | All remaining funds to freelancer. Status becomes `Completed`. |
| `PartialRefund` | `available - floor(available * 30 / 100)` | `floor(available * 30 / 100)` | 70/30 split with floor rounding. Status becomes `Completed`. |
| `Split(client_amount, freelancer_amount)` | `client_amount` | `freelancer_amount` | Custom split; both must be non-negative and sum to available. |

### Security Invariants

- **Conservation**: The payout values always sum to `available`. Negative splits are
  rejected with `InvalidDisputeSplit`.
- **Overflow protection**: All arithmetic uses checked operations; overflow returns
  `PotentialOverflow`.
- **Accounting integrity**: If `released + refunded > funded`, resolution fails with
  `AccountingInvariantViolated`.
- **Status determination**: `FinalRefund` sets status to `Refunded` only when the full
  funded amount has been refunded; otherwise `Completed`.

Test coverage is provided in `contracts/escrow/src/test/dispute.rs`.

## Planned Features

These features are not implemented entrypoints today:

- Two-step admin transfer: planned in
  [#318](https://github.com/Talenttrust/Talenttrust-Contracts/issues/318).
- Protocol fee treasury withdrawal: planned in
  [#314](https://github.com/Talenttrust/Talenttrust-Contracts/issues/314).
  Note: fee accumulation is now wired into `release_milestone` (issue #439).
  A `withdraw_protocol_fees` entrypoint remains unimplemented pending the
  dedicated fee-treasury issue.
- Governed parameter setter/readiness wiring: planned in
  [#323](https://github.com/Talenttrust/Talenttrust-Contracts/issues/323).
- `refund_unreleased_milestones` SAC refund path (the function exists but
  deferring the `token::Client::transfer` to the client until the
  refund-treasury issue picks up tracking).
- `migrate_state` / `StateV1` / `StateV2` migration flow: not implemented;
  tracked by this reconciliation issue
  [#341](https://github.com/Talenttrust/Talenttrust-Contracts/issues/341)
  until a dedicated implementation issue exists.

Any documentation that describes one of these items as available should be
treated as roadmap text, not live integration guidance.


### Milestone Approval and Revocation

Participants can approve milestone items prior to fund distribution payouts. If an authorization mistake is discovered prior to complete disbursement release configurations, the approving party can rescind authority.

#### `revoke_approval(contract_id: Address, caller: Address, milestone_index: u32)`
- **Authorization Required:** `caller.require_auth()`
- **Behavior:** Explicitly removes individual state flags (`client_approved` | `freelancer_approved` | `arbiter_approved`). When all structural components drop to `false`, temporary records are scrubbed entirely to maximize gas savings.
- **Errors raised:** `Error::MilestoneAlreadyReleased`, `Error::ApprovalRecordNotFound`.
