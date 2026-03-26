# Escrow Contract Documentation

This document describes escrow-specific controls and operational guidance.

## Emergency Pause Controls

The escrow contract includes admin-managed incident response controls:

- `initialize(admin)`: Sets the admin address once.
- `pause()`: Temporarily pauses state-changing functions.
- `unpause()`: Re-enables operations after a normal pause.
- `activate_emergency_pause()`: Activates emergency mode and hard-pauses operations.
- `resolve_emergency()`: Clears emergency mode and unpauses the contract.
- `is_paused()`: Read-only pause status.
- `is_emergency()`: Read-only emergency status.

### Guarded Functions

While paused, these state-changing flows revert with `ContractPaused`:

- `create_contract`
- `deposit_funds`
- `release_milestone`
- `issue_reputation`

## Dispute Lifecycle

The escrow contract supports a dispute lifecycle for funded contracts. Disputes are stored in persistent contract storage and transition the escrow agreement into `Disputed` status.

### Entry points

- `open_dispute(contract_id, initiator, reason)`
- `submit_dispute_evidence(contract_id, submitter, uri)`
- `resolve_dispute(contract_id, resolver, outcome)`
- `payout_dispute(contract_id)`

### Access control

- `open_dispute`:
  - Requires auth from `initiator`.
  - `initiator` must be either the `client` or the `freelancer`.
  - Contract must be `Funded`.
- `submit_dispute_evidence`:
  - Requires auth from `submitter`.
  - `submitter` must be either the `client` or the `freelancer`.
  - Evidence submission is blocked after the dispute is resolved.
- `resolve_dispute`:
  - Restricted to the pause-control `admin` set via `initialize(admin)`.
- `payout_dispute`:
  - Requires the dispute to be resolved first.
  - Marks the escrow contract `Completed` and records a payout state update.

### Threat-model notes

- Dispute resolution is admin-restricted so the protocol can enforce a clear incident-response and governance model.
- Mutating dispute operations respect `pause` and `activate_emergency_pause` fail-closed behavior.

### Error Codes

- `1` `AlreadyInitialized`
- `2` `NotInitialized`
- `3` `ContractPaused`
- `4` `NotPaused`
- `5` `EmergencyActive`

Note: Escrow lifecycle operations (including dispute flows) use Soroban contract errors (`Error(Contract, #N)`) defined in the escrow contract's `EscrowError` enum.

## Security Notes

- Admin-only controls: pause and emergency operations require authenticated admin.
- One-time initialization: admin cannot be replaced accidentally by repeated init calls.
- Emergency lock discipline: `unpause` is blocked while emergency mode is active.
- Fail-closed behavior: guarded functions revert whenever `paused == true`.

## Operational Playbook

1. Detect incident and call `activate_emergency_pause`.
2. Investigate and remediate root cause.
3. Validate mitigations in test/staging.
4. Call `resolve_emergency` to restore service.
5. Publish incident summary for ecosystem transparency.
