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

## Planned Governance Work

- Governed parameter setter/readiness wiring:
  [#323](https://github.com/Talenttrust/Talenttrust-Contracts/issues/323)
- Audit events for future fee/admin changes:
  [#340](https://github.com/Talenttrust/Talenttrust-Contracts/issues/340)
