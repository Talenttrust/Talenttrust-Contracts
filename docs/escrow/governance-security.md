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

## Planned Governance Work

- Governed parameter setter/readiness wiring:
  [#323](https://github.com/Talenttrust/Talenttrust-Contracts/issues/323)
- Audit events for future fee/admin changes:
  [#340](https://github.com/Talenttrust/Talenttrust-Contracts/issues/340)
