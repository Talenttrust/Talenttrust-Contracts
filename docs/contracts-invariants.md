# Contract Invariants

## Purpose and scope

This document records the invariants enforced by the contract source in this
repository. The Cargo workspace contains one contract crate,
`contracts/escrow`, and one Soroban contract, `Escrow`.

An invariant below is a property preserved by a public contract call that
returns successfully. A rejected call panics before it can commit a partial
Soroban transaction. Preconditions and authorization checks are included only
where they preserve an invariant.

The active module graph is the set of modules declared by
`contracts/escrow/src/lib.rs`: `amount_validation`, `approvals`, `deposit`,
`events`, `finalize`, `migration`, `milestones_consts`, `rollback`, `storage`,
`storage_validation`, `ttl`, `types`, `utils`, `create_contract`, `dispute`,
and `governance`. Files that are not declared in that graph are not enforcement
evidence, even if they contain an `impl Escrow` or tests.

The current source snapshot has compile-time inconsistencies, summarized under
[Source-audit limits and non-guarantees](#source-audit-limits-and-non-guarantees).
The tables therefore describe the guards and state transitions present in the
active source, not a claim that this revision currently produces a deployable
Wasm artifact.

## Initialization, administration, and pause state

| ID | Invariant | Relevant entrypoints | Enforcement |
| --- | --- | --- | --- |
| `INIT-01` | While `DataKey::Initialized` is live, initialization succeeds at most once. A successful call authenticates the selected admin, sets `Initialized = true`, stores that admin, initializes `NextContractId` to `1`, and marks the readiness checklist initialized. | `initialize`; observed by `get_admin`, `get_governance_admin`, `get_mainnet_readiness_info` | `contracts/escrow/src/lib.rs` - `Escrow::initialize` |
| `SETUP-01` | While `DataKey::SettlementToken` is live, the settlement token is write-once. Binding requires initialization, the stored admin's authentication, a token different from the escrow and admin addresses, and a successful `balance(escrow_address)` call on the candidate contract. | `bind_settlement_token`, deprecated alias `set_settlement_token`; observed by `get_settlement_token`, `is_settlement_token_bound`; consumed by all token-transfer entrypoints | `contracts/escrow/src/lib.rs` - `bind_settlement_token`, `read_settlement_token`, `write_settlement_token` |
| `ADMIN-01` | Privileged operations that are admin-gated use the current address stored at `DataKey::Admin`; after an accepted admin rotation, subsequent privileged calls require the new admin. | `bind_settlement_token`, `set_settlement_token`, `set_arbiter_config`, `set_max_settlement`, `set_protocol_fee_bps`, `set_max_milestones`, `set_governed_params`, `pause`, `unpause`, `activate_emergency_pause`, `resolve_emergency`, `set_reputation_config`, `reset_reputation_config`, `withdraw_protocol_fees`, `rollback_dispute`, `propose_governance_admin`, `cancel_governance_admin_proposal` | Named entrypoints in `contracts/escrow/src/lib.rs`; governance setters and proposal helpers in `contracts/escrow/src/governance.rs`; `contracts/escrow/src/rollback.rs` - `rollback_dispute_impl` |
| `ADMIN-02` | Admin transfer is two-step. The current admin authenticates a proposal, at least 34,560 ledger sequences must elapse, and the proposed address authenticates acceptance. Acceptance changes `DataKey::Admin` and removes the pending proposal; current-admin cancellation also removes it. | `propose_governance_admin`, `accept_governance_admin`, `cancel_governance_admin_proposal`, `get_pending_governance_admin`, `get_pending_governance_admin_proposed_at`, `get_pending_admin_proposed_at` | `contracts/escrow/src/governance.rs` - `propose_governance_admin_impl`, `accept_governance_admin_impl`, `cancel_governance_admin_proposal_impl`; `contracts/escrow/src/ttl.rs` - `ADMIN_ROTATION_MIN_DELAY_LEDGERS` |
| `PAUSE-01` | Public emergency-control transitions maintain `Emergency == true` only together with `Paused == true`: activation sets both, ordinary `unpause` refuses to clear pause during an emergency, and `resolve_emergency` clears both. | `pause`, `unpause`, `activate_emergency_pause`, `resolve_emergency`, `is_paused`, `is_emergency` | `contracts/escrow/src/lib.rs` - named entrypoints |
| `PAUSE-02` | Entrypoints that call `require_not_paused` cannot mutate state while either the pause or emergency flag is set. | `create_contract`, `deposit_funds`, `finalize_contract`, `rollback_dispute`, `propose_client_migration`, `accept_client_migration`, `approve_milestone_release`, `release_milestone`, `refund_unreleased_milestones`, `cancel_contract`, `set_reputation_config`, `issue_reputation`, `submit_work_evidence`, `batch_events`, `emit_events_batch`, `events_batch`, `emit_event`, `raise_dispute`, `resolve_dispute` | `contracts/escrow/src/finalize.rs` - `Escrow::require_not_paused`; call sites in `lib.rs`, `create_contract.rs`, `migration.rs`, `rollback.rs`, and `finalize.rs` |
| `READINESS-01` | Readiness flags are monotonic through current public writers: `initialize` sets `initialized`, `set_governed_params` sets `governed_params_set`, and both emergency-control actions set `emergency_controls_enabled`; no public entrypoint clears one. | `initialize`, `set_governed_params`, `activate_emergency_pause`, `resolve_emergency`, `get_mainnet_readiness_info` | Named entrypoints in `contracts/escrow/src/lib.rs` and `contracts/escrow/src/governance.rs` |

Pause is selective. Token binding, governance setters, reputation-configuration
reset, and admin proposal/acceptance/cancellation do not call
`require_not_paused` and remain callable while paused or in emergency.
`withdraw_protocol_fees` checks `Paused` directly; a publicly reachable
emergency also sets `Paused`, so it is blocked in that state.

Initialization is also selective. In particular, `create_contract` does not
require initialization. Creating state before `initialize` and then resetting
`NextContractId` to `1` during initialization is not a supported uniqueness
guarantee. A zero-funded `Created` contract can also be created and cancelled
before initialization. Milestone approval/release/refund, cancellation, and
finalization lack direct initialization guards, although funded-state paths are
normally reached through the initialization-gated `deposit_funds`.

There is no deployer/factory authorization on `initialize`: the first
successful invocation chooses an address and proves that address's
authorization. There is also no generic RBAC, role-grant, or role-revoke API.
Participant roles come from each stored `Contract`; protocol administration
comes from `DataKey::Admin`.

`activate_emergency_pause` calls `admin.require_auth()` only when
`Initialized` is true. On clean pre-initialization storage it still fails
because no `Admin` key exists.

A governance-admin proposal may overwrite an existing proposal, may nominate
the current admin, and has no maximum acceptance window. Acceptance requires
the proposed admin's authentication after the delay; it does not require the
current admin to co-sign. `DataKey::PendingAdmin` is persistent and has no
explicit TTL-renewal path.

The readiness checklist is informational. No lifecycle or money-flow
entrypoint requires all of its flags to be true.

## Contract creation and funding

| ID | Invariant | Relevant entrypoints | Enforcement |
| --- | --- | --- | --- |
| `CREATE-01` | The authenticated client and freelancer are distinct. An assigned arbiter is distinct from both. `ArbiterOnly` and `ClientAndArbiter` release modes require an arbiter. | `create_contract` | `contracts/escrow/src/create_contract.rs` - `Escrow::create_contract` |
| `CREATE-02` | A successfully created schedule is non-empty, contains at most 10 milestones, and every amount is in `1..=10_000_000_000_000` stroops. Addition is checked. If `GovernedParameters` exists, the schedule total cannot exceed its positive `max_escrow_total_stroops`; without it, the source falls back to `i128::MAX`. | `create_contract`; cap written by `set_governed_params` | `contracts/escrow/src/create_contract.rs` - `create_contract`; `contracts/escrow/src/amount_validation.rs` - `validate_single_amount`, `validate_amount_array`, `validate_milestone_amounts` |
| `CREATE-03` | A new record starts in `Created` with `total_deposited`, `funded_amount`, `released_amount`, and `refunded_amount` equal to zero. Each new milestone starts unfunded, unreleased, unrefunded, without evidence, and without a deadline. An occupied contract ID is not overwritten, and advancing the counter uses checked addition. | `create_contract`; observed by contract and milestone getters | `contracts/escrow/src/create_contract.rs` - `create_contract`, `next_contract_id` |
| `DEPOSIT-01` | A deposit is positive, within the single-amount limit, supplied by the stored client, and accepted only from `Created` or `PartiallyFunded`. Checked accumulation cannot exceed the milestone total. Exact full funding produces `Funded`; a smaller total produces `PartiallyFunded`. | `deposit_funds`; observed by `get_contract`, `get_contract_summary`, `get_refundable_balance` | `contracts/escrow/src/lib.rs` - `deposit_funds`; `contracts/escrow/src/deposit.rs` - `validate_deposit`, `apply_validated_deposit`; `contracts/escrow/src/storage_validation.rs` - `validate_stroop_amount` |
| `DEPOSIT-02` | Through the active creation and deposit writers, `total_deposited == funded_amount`: both start at zero and every successful deposit adds the same checked amount to both. | `create_contract`, `deposit_funds` | `contracts/escrow/src/create_contract.rs` - initial record; `contracts/escrow/src/deposit.rs` - `apply_validated_deposit` |
| `ACCOUNTING-01` | State reachable through the active accounting writers keeps `funded_amount`, `released_amount`, and `refunded_amount` non-negative and preserves `released_amount + refunded_amount <= funded_amount`. Dispute resolution and cancellation consume the entire remaining accounting balance and make the relation an equality. | `create_contract`, `deposit_funds`, `release_milestone`, `refund_unreleased_milestones`, `cancel_contract`, `resolve_dispute`; observed by contract and balance readers | Creation/deposit writers in `create_contract.rs` and `deposit.rs`; balance guards and updates in `lib.rs`; `contracts/escrow/src/dispute.rs` - `resolution_payouts` |

`deposit_funds` additionally requires initialization, an unpaused/non-emergency
state, and a bound settlement token. It invokes the bound token's
`transfer(client, escrow, amount)` method with the accepted amount. The
repository does not prove that a duck-typed external token implements honest
SAC semantics.

## Milestone settlement and custody accounting

| ID | Invariant | Relevant entrypoints | Enforcement |
| --- | --- | --- | --- |
| `APPROVAL-01` | Approval records are scoped to `(contract_id, milestone_index)`. A duplicate flag for the same role is rejected, an absent or expired record fails release closed, and a successful release removes its record. | `approve_milestone_release`, `release_milestone`, `get_milestone_approvals`, authorization-record readers | `contracts/escrow/src/approvals.rs` - `approve_milestone`, `check_approvals`, `clear_approvals`; `contracts/escrow/src/ttl.rs` - approval TTL constants |
| `RELEASE-01` | Release requires status exactly `Funded`, a valid unsettled index, sufficient approval flags, an authenticated release caller allowed by the stored `ReleaseAuthorization`, and sufficient accounting balance. | `approve_milestone_release`, `release_milestone` | `contracts/escrow/src/lib.rs` - `release_milestone`; `contracts/escrow/src/approvals.rs` - `check_approvals` |
| `RELEASE-02` | In one successful release call, the source invokes the bound token's `transfer` with the net milestone amount and freelancer destination, writes `released = true` and the gross funding amount to `DataKey::Milestones(id)`, adds the net amount to `contract.released_amount` with checked arithmetic, and adds the fee to the global accumulated-fee counter. | `release_milestone`; fee configured by `set_protocol_fee_bps`; observed by `get_accumulated_protocol_fees` | `contracts/escrow/src/lib.rs` - `release_milestone`, `calculate_protocol_fee`, `read_protocol_fee_bps`; `contracts/escrow/src/ttl.rs` - `store_milestones` |
| `REFUND-01` | A refund call is authenticated by the stored client and contains a non-empty, duplicate-free set of valid, unreleased, unrefunded milestone indices. The contract status must be `Created`, `Funded`, or `Disputed`, and the derived remaining balance must cover the total refund. | `refund_unreleased_milestones` | `contracts/escrow/src/lib.rs` - `refund_unreleased_milestones`; `contracts/escrow/src/finalize.rs` - `require_active_contract` |
| `REFUND-02` | A milestone with `Some(deadline)` is refundable only when the ledger timestamp is strictly greater than the deadline. Missing contracts, missing milestones, out-of-range indices, released milestones, and milestones without a deadline are reported as not overdue by the read-only predicate. | `is_milestone_overdue`, `refund_unreleased_milestones` | `contracts/escrow/src/lib.rs` - both entrypoints; `contracts/escrow/src/utils.rs` - `now_seconds` |
| `LIFECYCLE-01` | When a settlement call's function-local milestone vector has every entry released or refunded, it sets a terminal status: all-refunded becomes `Refunded`; otherwise it becomes `Completed` and one pending reputation credit is added. Release performs this check on the composite-key vector it reloads; refund performs it on `DataKey::Milestones(id)`. | `release_milestone`, `refund_unreleased_milestones` | `contracts/escrow/src/lib.rs` - completion branches and `grant_pending_reputation_credit`; `contracts/escrow/src/ttl.rs` - refund milestone load |
| `CANCEL-01` | Cancellation is authenticated by the stored client, allowed only from `Created` or `Funded` with zero released balance, credits the full remaining accounting balance as refunded, and sets status to `Cancelled`. | `cancel_contract` | `contracts/escrow/src/lib.rs` - `cancel_contract`; `contracts/escrow/src/finalize.rs` - `require_active_contract` |

Release-caller authentication and approval-record authentication are different.
`release_milestone` authenticates its caller and enforces the mode:

| Release mode | Authenticated caller allowed to release | Approval flags required |
| --- | --- | --- |
| `ClientOnly` | client | client |
| `ArbiterOnly` | assigned arbiter | arbiter |
| `ClientAndArbiter` | client or assigned arbiter | client or arbiter |
| `MultiSig` | client or freelancer | client and freelancer |

However, `approve_milestone_release` and
`approvals.rs::approve_milestone` do not call `caller.require_auth()`. They
only compare the supplied address with stored participant addresses. Thus the
source guarantees the required booleans, but not that the participants
authenticated those approvals; in particular, `MultiSig` is not an
authenticated two-party approval guarantee.

The auth-free `get_milestone_approvals` reader can renew a live approval's TTL,
including while paused or in emergency. Pause therefore does not freeze every
storage mutation.

`release_milestone` also applies a local pre-commit guard:

```text
contract.released_amount
  + contract.refunded_amount
  + AccumulatedProtocolFees
  <= contract.funded_amount
```

Here `released_amount` is the net payout and `AccumulatedProtocolFees` is the
global, not per-contract, counter. A later release for another contract can
change that global counter, so this check is not a persistent per-contract
accounting invariant.

Newly created milestones always have `deadline = None`, and no active public
entrypoint writes `Some(deadline)`. The timeout branch in `REFUND-02` therefore
applies only to legacy or directly injected state, not to a milestone created
through the current public API. A `None` deadline skips the overdue requirement
and permits immediate refund when the other refund preconditions hold.

There is no durable one-shot or mutually exclusive milestone-flag invariant in
the current source. Creation and several readers use the composite key
`(DataKey::Contract(id), Symbol("milestones"))`, while settlement uses
`DataKey::Milestones(id)`. `submit_work_evidence` reads the composite vector and
writes it to `DataKey::Milestones(id)`, which can overwrite release/refund
flags. `release_milestone` also reloads the composite vector before writing the
settlement key. A later successful call can therefore reset a flag and settle
the same milestone again if contract-level accounting still has enough value.

## Disputes, rollback, and finalization

| ID | Invariant | Relevant entrypoints | Enforcement |
| --- | --- | --- | --- |
| `DISPUTE-01` | A dispute opens only on an initialized, unpaused, unfinalized `Funded` or `PartiallyFunded` contract with an assigned arbiter, and only an authenticated stored client or freelancer may open it. Success changes status to `Disputed`. | `raise_dispute` | `contracts/escrow/src/lib.rs` - `raise_dispute`, `require_initialized`; `contracts/escrow/src/finalize.rs` - `require_active_contract` |
| `DISPUTE-02` | Resolution is accepted only from status `Disputed`, only before finalization, and only with authentication by the exact assigned arbiter. | `resolve_dispute` | `contracts/escrow/src/lib.rs` - `resolve_dispute` |
| `DISPUTE-03` | Resolution arithmetic conserves the remaining accounting balance: `client_payout + freelancer_payout == funded_amount - released_amount - refunded_amount`. Custom legs must be non-negative and sum exactly to that balance; arithmetic overflow and negative availability are rejected. | `resolve_dispute` | `contracts/escrow/src/dispute.rs` - `resolution_payouts` |
| `DISPUTE-04` | After accounting resolution, status is `Refunded` exactly when cumulative refunds equal funded amount; otherwise it is `Completed`, which grants a pending reputation credit. | `resolve_dispute` | `contracts/escrow/src/dispute.rs` - `final_status_after_resolution`; `contracts/escrow/src/lib.rs` - resolution state writes |
| `FINAL-01` | Finalization is write-once, requires authentication by the stored client, freelancer, or assigned arbiter, and is allowed only in `Completed` or `Disputed`. The finalization record is a snapshot that no public writer overwrites. | `finalize_contract`, `get_finalization_record` | `contracts/escrow/src/finalize.rs` - `finalize_contract_impl`, `require_not_finalized`, `summarize_contract` |
| `SCHEMA-01` | Public contract summaries and finalization snapshots carry schema version `1`. This versions the returned summary shape, not the underlying `Contract` storage layout. | `get_contract_summary`, `finalize_contract`, `get_finalization_record` | `contracts/escrow/src/types.rs` - `CONTRACT_SUMMARY_SCHEMA_VERSION`; `contracts/escrow/src/lib.rs` - `get_contract_summary`; `contracts/escrow/src/finalize.rs` - `summarize_contract` |
| `ROLLBACK-01` | If a rollback snapshot exists, rollback is admin-authenticated and single-use. It succeeds only for an unfinalized `Disputed` contract whose current contract and milestones exactly equal the stored pre-dispute snapshot except for the status change; it restores only the prior `Funded`/`PartiallyFunded` status and removes the snapshot. | `rollback_dispute` | `contracts/escrow/src/rollback.rs` - `rollback_dispute_impl`, `DisputeRollbackRecord` |

The public `raise_dispute` implementation does not store a rollback snapshot.
Only the unused helper `dispute.rs::raise_dispute_impl` does so. Consequently,
`ROLLBACK-01` is a conditional guard, but no normal public dispute-opening
sequence creates the record required for a successful rollback.

`resolve_dispute` updates accounting fields but performs no settlement-token
transfer and does not update milestone flags. Its `PartialRefund` arithmetic is
hard-coded to a 30% freelancer share and does not read the stored arbiter
configuration.

Finalization freezes only entrypoints that check the finalization record or
whose status preconditions exclude `Completed`/`Disputed`. It does not make all
live state immutable: for example, `issue_reputation` may update a completed
contract after its finalization snapshot was written.

`finalize.rs::summarize_contract` reads the composite milestone vector, not
`DataKey::Milestones(id)`. A finalization snapshot can therefore have terminal
contract accounting while reporting stale milestone flags and a released count
that disagrees with the settlement path.

## Client migration

| ID | Invariant | Relevant entrypoints | Enforcement |
| --- | --- | --- | --- |
| `MIGRATION-01` | A live proposal is unique per contract, temporary, and created only by the authenticated current client. The proposed address differs from the current client and freelancer, and the contract must be unfinalized and outside `Completed`, `Cancelled`, `Refunded`, and `Disputed`. | `propose_client_migration`; observed by `has_pending_client_migration`, `get_pending_client_migration` | `contracts/escrow/src/migration.rs` - `propose_client_migration_impl`, `require_migration_allowed`, `pending_migration_exists`; `contracts/escrow/src/ttl.rs` - migration TTL constants |
| `MIGRATION-02` | Acceptance requires authentication by the exact proposed address, a live proposal, an allowed unfinalized status, and a proposal whose recorded current client still equals the contract's stored client. | `accept_client_migration` | `contracts/escrow/src/migration.rs` - `accept_client_migration_impl` |

`accept_client_migration_impl` stops after validation and event emission. It
does not assign or persist `contract.client` and does not remove the pending
proposal. Therefore client transfer, proposal consumption, and replay
prevention are not invariants. The `cancel_client_migration` method in
`migration.rs` is in an ordinary inherent `impl`, has no root wrapper, and is
not a Soroban contract entrypoint.

`has_pending_client_migration` and `get_pending_client_migration` inspect only
temporary-key liveness. They do not verify contract existence, status,
initialization, pause, or finalization.

## Reputation and work evidence

| ID | Invariant | Relevant entrypoints | Enforcement |
| --- | --- | --- | --- |
| `REPUTATION-01` | Reputation is issued at most once per contract, only after `Completed`, only by the authenticated stored client, and only when client and freelancer differ. The rating and non-empty comment must satisfy the current reputation configuration, and the freelancer must have a positive pending credit. | `issue_reputation`; observed by reputation and comment getters | `contracts/escrow/src/lib.rs` - `issue_reputation` |
| `REPUTATION-02` | A successful issuance atomically marks the contract and per-contract marker issued, consumes one pending credit, increments the freelancer's completed-contract count, adds the rating, stores the last rating, and stores the comment. | `issue_reputation`, `get_reputation`, `get_reputation_comment`, `get_pending_reputation_credits`, `get_average_rating` | `contracts/escrow/src/lib.rs` - `issue_reputation` and named readers |
| `REPUTATION-03` | The average reader returns checked, floor-rounded fixed-point arithmetic `total_rating * 10_000 / completed_contracts`, or `None` for a missing record, zero divisor, or arithmetic failure. | `get_average_rating` | `contracts/escrow/src/lib.rs` - `get_average_rating` |
| `EVIDENCE-01` | Work evidence is accepted only from the authenticated stored freelancer while the contract is initialized, unpaused, unfinalized, and exactly `Funded`. The milestone must be valid and unsettled, and evidence is at most 256 bytes. | `submit_work_evidence`; observed by `get_work_evidence` | `contracts/escrow/src/lib.rs` - `submit_work_evidence`, `get_work_evidence` |

Evidence may be empty and may overwrite earlier evidence; it is not append-only.
Reputation index membership is not independently checked: `issue_reputation`
appends when the loaded record's `completed_contracts` is zero, and
`get_reputations_page` substitutes a default record when indexed data is
missing. Independent TTL expiry means index uniqueness and completeness are not
invariants.

## Configuration and protocol fees

| ID | Invariant | Relevant entrypoints | Enforcement |
| --- | --- | --- | --- |
| `CONFIG-01` | The canonical release-fee value stored at `DataKey::ProtocolFeeBps` is written only by an authenticated admin and is bounded to `0..=10_000`. Release fee calculation uses checked multiplication and floor division by 10,000. | `set_protocol_fee_bps`, `get_protocol_fee_bps`, `calculate_protocol_fee`, `release_milestone` | `contracts/escrow/src/governance.rs` - `set_protocol_fee_bps`; `contracts/escrow/src/storage_validation.rs` - `validate_protocol_fee_bps`; `contracts/escrow/src/lib.rs` - fee helpers and release |
| `CONFIG-02` | A successful governed-parameter write requires admin authentication, `protocol_fee_bps <= 10_000`, and `max_escrow_total_stroops > 0`; it also marks governed parameters set in the readiness checklist. Creation consumes the stored maximum escrow total. | `set_governed_params`, `get_governed_parameters`, `create_contract` | `contracts/escrow/src/governance.rs` - `set_governed_params`; `contracts/escrow/src/storage_validation.rs` - `validate_escrow_total_cap`; `contracts/escrow/src/create_contract.rs` - cap read |
| `CONFIG-03` | Stored arbiter split configuration, when written, has each leg at most 10,000 bps and both legs sum exactly to 10,000. | `set_arbiter_config`, `get_arbiter_config` | `contracts/escrow/src/lib.rs` - `set_arbiter_config`; `contracts/escrow/src/dispute.rs` - configuration storage helpers |
| `CONFIG-04` | Reputation configuration written through the public setter satisfies `1 <= min_rating <= max_rating <= 10` and `1 <= max_comment_bytes <= 1,000`. Reset restores `1..=5` ratings and a 200-byte comment limit. | `set_reputation_config`, `reset_reputation_config`, `get_reputation_config`, `issue_reputation` | `contracts/escrow/src/lib.rs` - configuration entrypoints; `contracts/escrow/src/storage_validation.rs` - `validate_reputation_config_params`; `contracts/escrow/src/types.rs` - `ReputationConfig::default` |
| `CONFIG-05` | The stored maximum settlement value, when successfully set, is in `1..=100`; an absent value reads as 10. | `set_max_settlement`, `get_max_settlement`, `get_bounds` | `contracts/escrow/src/lib.rs` - named functions and `effective_max_settlement` |
| `FEE-01` | Tracked accumulated fees cannot be withdrawn below zero through `withdraw_protocol_fees`: the authenticated current admin must request a positive, bounded amount no greater than the stored counter. Success subtracts that amount and invokes the bound token's `transfer` with the same amount. | `withdraw_protocol_fees`, `get_accumulated_protocol_fees` | `contracts/escrow/src/lib.rs` - named functions |

`GovernedParameters.protocol_fee_bps` is separate from
`DataKey::ProtocolFeeBps` and is not read by release, so
`set_governed_params` does not change the effective release fee.
`set_arbiter_config` does not affect the hard-coded partial-dispute split.
No batch-settlement entrypoint consumes `MaxSettlement`. The fee-withdrawal
destination is arbitrary and selected by the admin; there is no stored treasury
address, treasury allowlist, or withdrawal timelock.

`get_bounds().max_total_escrow_stroops` is not the default cap enforced by
`create_contract`: absent `GovernedParameters`, creation uses `i128::MAX`.
`set_max_milestones` is also not consumed by creation and currently references
the nonexistent `DataKey::MaxMilestones`.

The public `calculate_protocol_fee` helper does not itself validate an
arbitrary caller-supplied amount or basis-point value. The `0..=10_000` bound
applies when release uses the canonical value written by
`set_protocol_fee_bps`.

## Temporary storage and bounded reads

| ID | Invariant | Relevant entrypoints | Enforcement |
| --- | --- | --- | --- |
| `TTL-01` | Missing or expired temporary approval records fail closed as insufficient approvals. A successful approval requests a 120,960-ledger TTL; the approval getter can renew a live record near expiry; release removes it. | `approve_milestone_release`, `get_milestone_approvals`, `release_milestone` | `contracts/escrow/src/approvals.rs`; `contracts/escrow/src/ttl.rs` |
| `TTL-02` | Missing or expired client-migration proposals fail closed. A successful proposal requests a 362,880-ledger TTL and records a saturating informational expiry ledger. | client-migration proposal, acceptance, and readers | `contracts/escrow/src/migration.rs`; `contracts/escrow/src/ttl.rs` |
| `READ-01` | Authorization-record pages contain at most 50 entries and reputation pages at most 100; zero limits and out-of-range starts return empty vectors. | authorization-record readers, `get_reputations_page` | `contracts/escrow/src/approvals.rs` - `get_authorization_records`; `contracts/escrow/src/types.rs` - `MAX_PAGINATION_LIMIT`; `contracts/escrow/src/lib.rs` - `get_reputations_page`, `PAGE_CEILING` |

`get_approval_deadline` does not expose the actual remaining TTL. It checks
whether the record is live and then returns the current ledger sequence plus a
full approval TTL.

Persistent records do not have a repository-wide permanence invariant. TTL
helpers request renewal to 518,400 ledgers below a 120,960-ledger threshold,
but renewal is applied selectively. Milestone renewal targets
`DataKey::Milestones(id)`, not the composite key written by creation, and
configuration, indexes, reputation records, finalization, admin,
initialization, and settlement-token keys lack consistent renewal. Soroban may
archive expired persistent entries and deletes expired temporary entries. The
write-once and uniqueness properties above are therefore scoped to the relevant
keys remaining live.

## Event correspondence

The active entrypoint bodies provide these successful-call postconditions:

- `raise_dispute` writes `Disputed` before emitting `("dispute", "opened")`
  and `("dsp_index", "raised")`.
- `resolve_dispute` places `("dispute", "resolved")` and
  `("dsp_index", "settled")` after its accounting writes, subject to the
  current `DisputeInfo`/tuple compile mismatch.
- `propose_client_migration` emits `client_migration_proposed`, and
  `accept_client_migration` emits `client_migration_accepted`. The latter event
  is not evidence that client state changed, because the implementation makes
  no such write.
- `issue_reputation` emits no event.

The helper emitters in `contracts/escrow/src/events.rs` are not called by these
production entrypoints and are not enforcement evidence.

## Source-audit limits and non-guarantees

The following findings delimit the invariants above:

1. **The active source currently has compile blockers.** Examples include
   duplicate error variants/discriminants, missing `MilestoneEntry`,
   `EventInput`, `MAX_EVENT_BATCH_SIZE`, and `status_index`, missing root
   constant/type re-exports, nonexistent `DataKey::MaxMilestones`, and the
   public `resolve_dispute` treating `DisputeInfo` as a tuple.

2. **There is no canonical milestone storage key.** Creation, deposit, several
   getters, and finalization use
   `(DataKey::Contract(id), Symbol("milestones"))`; `ttl::load_milestones`,
   `ttl::store_milestones`, approvals, and several mutation paths use
   `DataKey::Milestones(id)`. Consequently the current public creation path
   does not establish the storage shape expected by release/refund helpers.

3. **There is no repository-enforced token-balance conservation equation.**
   Custody is pooled in one external token contract, accumulated fees are
   global, and no entrypoint reconciles the actual token balance with internal
   records. `resolve_dispute` changes accounting without transferring tokens.

4. **Some counters lack overflow protection.** Reputation counts, total
   ratings, and pending-credit increments use
   unchecked `+=`/`+ 1` arithmetic, so the source does not establish an
   unbounded overflow-safety invariant for those counters.

5. **The code does not implement a checks-effects-interactions ordering
   guarantee.** Deposit, release, refund, and cancellation call the external
   token before persisting their corresponding accounting effects. Transaction
   rollback on failure is a Soroban host property, not a local reentrancy guard.

6. **The token probe is an interface call, not an asset-authenticity proof.**
   Any contract that successfully implements the called `balance` interface can
   pass it. The source directly invokes the call and does not translate a panic
   into the documented `InvalidSettlementToken` error.

7. **Generic events are not state attestations.** Any authenticated address can
   call `emit_event` or its batch variants with arbitrary topic/data; no
   participant or admin role is required.

8. **No upgrade invariant exists.** There is no active Wasm-upgrade, deployer,
   Wasm-hash, general state-migration, or reputation-storage-migration
   entrypoint. Likewise, there are no vault, allocation-strategy, nester, or
   separate treasury contracts in this workspace.

9. **Undeclared source files do not enforce contract behavior.** In particular,
   `authorization.rs`, `contracts.rs`, `milestones.rs`, `release.rs`,
   `refund.rs`, `refund_impl.rs`, `settlement.rs`, and
   `reputation_migration.rs` are not in the active module graph and are not
   cited above.

## Supporting tests

Tests supplement, but do not replace, the source audit. Representative wired
suites include:

- `contracts/escrow/src/test/mainnet_readiness.rs` for initialization and
  readiness flags;
- `contracts/escrow/src/test/input_sanitization_identities.rs`,
  `input_sanitization_amounts.rs`, and `input_bounds_validation.rs` for creation
  and amount guards;
- `contracts/escrow/src/test/deposit.rs`, `release.rs`, `refund.rs`,
  `cancel_contract.rs`, and `rollback.rs` for lifecycle paths;
- `contracts/escrow/src/test/approval_expiry.rs` and
  `release_authorization.rs` for approval flags and release modes;
- `contracts/escrow/src/test/pause_controls.rs`,
  `emergency_controls.rs`, and `governance_pause_matrix.rs` for selective
  pause behavior;
- `contracts/escrow/src/test/dispute.rs` and `disputes_auth_matrix.rs` for
  dispute arithmetic, roles, and transitions;
- `contracts/escrow/src/test/persistence.rs` for finalization and getters; and
- `contracts/escrow/src/test/reputation.rs` and
  `reputation_config_setter.rs` for reputation rules.

Some relevant-looking test files are not declared by
`contracts/escrow/src/test/mod.rs`, and some wired tests contradict or ignore
the active implementation. Those tests are not used as sole evidence for any
invariant in this document.
