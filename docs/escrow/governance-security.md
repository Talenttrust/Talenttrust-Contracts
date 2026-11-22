# Escrow Governance Security

The live escrow contract has a single operational admin initialized by
`initialize(admin)`. That admin can pause, unpause, activate emergency pause,
resolve emergency mode, and hand off the role via a two-step, timelocked
transfer (see below).

## Implemented Admin Controls

- `initialize(admin) -> bool`
- `get_admin() -> Option<Address>`
- `pause() -> bool`
- `unpause() -> bool`
- `activate_emergency_pause() -> bool`
- `resolve_emergency() -> bool`
- `is_paused() -> bool`
- `is_emergency() -> bool`

### Two-step admin transfer

A single-call admin transfer is a well-known footgun: a typo'd address or a
compromised admin key hands over the whole contract irrevocably. Instead,
rotation is propose → (wait out a timelock) → accept, with a cancel escape
hatch and a hard expiry so a forgotten proposal can't be accepted long after
the fact. See [`docs/escrow/`](.) and the crate-level docs on
`escrow::governance` for the full design rationale.

- `propose_admin(new: Address) -> bool` — current admin only. Stores `new`
  under `PendingAdmin` with the current ledger sequence. Rejects proposing the
  current admin itself (`Error::CannotProposeSelf`). A second call overwrites
  any existing pending proposal.
- `accept_admin() -> bool` — the *proposed* address must authorize. Fails with
  `Error::TimelockNotElapsed` before `ADMIN_ROTATION_MIN_DELAY_LEDGERS` (~2
  days) have elapsed since the proposal, and with
  `Error::AdminProposalExpired` after `ADMIN_ROTATION_PROPOSAL_TTL_LEDGERS`
  (~9 days) have elapsed — a panic rolls back all state, so an expired
  proposal is left in place, not silently cleared.
- `cancel_admin() -> bool` — current admin only. Clears a pending proposal at
  any time, expired or not, with no timelock of its own.
- `get_pending_admin() -> Option<Address>` — the proposed address, if any.
- `get_pending_admin_proposed_at() -> Option<u32>` (alias:
  `pending_admin_proposed_at`) — the ledger sequence the pending proposal was
  made at, so off-chain tooling can compute the remaining timelock/expiry.

Every transition clears or overwrites `PendingAdmin`, so an accept can never
be replayed against a cancelled or already-consumed proposal — it finds
nothing pending and fails with `Error::InvalidState`.

All mutating admin controls require the stored admin's (or, for `accept_admin`,
the proposed admin's) Soroban authorization.

### Concurrent clients and retries

Transactions execute atomically in serialized order, but a signed request can
arrive after the pending proposal has been replaced. Use the additive checked
entrypoints to bind intent to an observed admin-rotation revision:

- `get_admin_rotation_revision() -> u64`
- `propose_admin_checked(new: Address, expected_revision: u64) -> bool`
- `accept_admin_checked(expected_revision: u64) -> bool`
- `cancel_admin_checked(expected_revision: u64) -> bool`
- `recover_admin_proposal_checked(expected_revision: u64) -> bool`

Read the revision before reading the pending proposal and confirming the action.
For a consistent multi-query snapshot, read the revision again after the proposal
and repeat the reads if it changed. Include that revision in the authorized call.
All existing authorization, delay and expiry rules still apply. Recovery requires
the current admin and an elapsed delay strictly greater than the proposal TTL;
acceptance remains allowed at exactly the TTL.

Every successful proposal, acceptance, cancellation or recovery advances the
revision, including calls to the legacy entrypoints. Exactly one of two requests
using the same revision can succeed. Repeated requests fail with `StaleNonce`
without changing storage or emitting successful events, including replacement by
the same address in the same ledger. On `StaleNonce`, read and review the new state;
do not blindly substitute a newer revision into an old action. A successful call
whose response was lost can be diagnosed from the current state and events rather
than replayed as a new mutation.

The revision is stored in contract instance storage so it cannot independently
expire while the instance remains live. Missing revision storage on upgrade reads
as zero; the existing pending-proposal encoding is unchanged. The counter never
wraps: mutation at `u64::MAX` fails with `PotentialOverflow`, leaving state intact.
Each successful mutation adds an `admin_rotation_revision` event containing the
new revision. Existing admin event topics and payloads remain unchanged.

The legacy methods retain their signatures and latest-state behavior. Clients
must migrate to checked methods for stale-intent protection; legacy calls do not
gain that protection automatically. Existing deployed proposals can be operated
on at revision zero after upgrade. See `tests/governance_concurrency.rs` for both
serialized race orders, migration, retries, authorization and boundary checks.

### Timelocked protocol fee changes

Protocol fee increases are protected by a 48-hour timelock (`PARAM_DELAY_SECS = 172_800`,
matching `UPGRADE_DELAY_SECS`), following the same schedule → execute / cancel pattern as
WASM upgrades:

- `schedule_fee_change(fee_bps: u32, treasury: Address)` — admin only. Validates
  `fee_bps <= MAX_FEE_BPS` (`FeeTooHigh`) and `treasury != current_contract_address()`
  (`InvalidParties`), stores `PendingFeeChange { fee_bps, treasury, eta }` with
  `eta = now + PARAM_DELAY_SECS`, and emits `("fee_sched",) -> (fee_bps, treasury, eta)`.
  Re-scheduling while a change is pending resets the timer.
- `execute_fee_change()` — admin only. Fails with `NoPendingUpgrade` if nothing is
  pending or `UpgradeNotReady` if `now < eta`. Clears the pending record, updates
  `FeeBps` and `Treasury`, and emits `("fee_exec",) -> (fee_bps, treasury)`.
- `cancel_fee_change()` — admin only. Fails with `NoPendingUpgrade` if nothing is
  pending. Clears the pending record and emits `("fee_cncl",) -> (fee_bps, treasury)`.
- `get_pending_fee_change() -> Option<PendingFeeChange>` — view returning the currently
  scheduled fee change and its `eta`, if any.

Existing escrows keep the fee rate snapshotted at creation (`get_escrow_fee(id)`);
only newly created escrows pick up the updated rate.

**Compatibility choice for `set_fee`:** The direct `set_fee(fee_bps, treasury)` setter
remains available **only for non-increasing fees** (`fee_bps <= current_fee_bps`),
allowing the admin to lower or zero out fees immediately without waiting 48 hours.
Any attempt to increase `fee_bps` via `set_fee` is rejected with `UpgradeNotReady`.

**Why `set_panel` is exempt:** The arbiter panel (`set_panel`) is intentionally kept
immediate so the admin can rapidly replace an unresponsive or compromised arbiter
during an active high-value dispute. Every panel update emits `("panel",) -> (len, threshold)`
for off-chain monitoring.

## Planned Governance Work

- Governed parameter setter/readiness wiring:
  [#323](https://github.com/Talenttrust/Talenttrust-Contracts/issues/323)
- Audit events for future fee/admin changes:
  [#340](https://github.com/Talenttrust/Talenttrust-Contracts/issues/340)
