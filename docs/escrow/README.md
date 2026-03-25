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

### Error Codes

- `1` `AlreadyInitialized`
- `2` `NotInitialized`
- `3` `ContractPaused`
- `4` `NotPaused`
- `5` `EmergencyActive`

## Escrow Creation Boundaries

To prevent out-of-gas or infinite-loop denial of service attacks, the escrow contract enforces creation limits:
- **Maximum Milestone Count**: Hard-capped by `ProtocolParameters.max_milestones` (defaults to 16).
- **Maximum Contract Size/Funding**: Total escrow amounts are bounded (e.g., `< 1,000,000,000,000` stroops) to prevent integer overflows or massive storage requirements footprint.

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
