# Escrow Governance Security

The live escrow contract has a single operational admin initialized by
`initialize(admin)`. That admin can pause, unpause, activate emergency pause, and
resolve emergency mode.

## Implemented Admin Controls

- `initialize(admin) -> bool`
- `get_admin() -> Option<Address>`
- `pause() -> bool`
- `unpause() -> bool`
- `activate_emergency_pause() -> bool`
- `resolve_emergency() -> bool`
- `is_paused() -> bool`
- `is_emergency() -> bool`

- `get_pending_governance_admin() -> Option<Address>` — reads the proposed
  address from the pending admin proposal (correctly decodes the
  [`PendingAdminProposal`] struct, not a bare `Address`)
- `get_pending_governance_admin_proposed_at() -> Option<u32>` — returns the
  ledger sequence when the pending admin was proposed (alias for
  `get_pending_admin_proposed_at`)
- `get_pending_admin_proposed_at() -> Option<u32>` — returns the ledger sequence
  when the pending admin was proposed

All mutating admin controls require the stored admin's Soroban authorization.
There is no live admin transfer entrypoint.

## Planned Governance Work

- Two-step admin transfer:
  [#318](https://github.com/Talenttrust/Talenttrust-Contracts/issues/318)
- Governed parameter setter/readiness wiring:
  [#323](https://github.com/Talenttrust/Talenttrust-Contracts/issues/323)
- Audit events for future fee/admin changes:
  [#340](https://github.com/Talenttrust/Talenttrust-Contracts/issues/340)

Until those issues land, operational key management for the initialized admin is
an off-chain process.
