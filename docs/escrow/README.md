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
- `approve_milestone_release`
- `release_milestone`
- `issue_reputation`

### Error Codes

- `1` `AlreadyInitialized`
- `2` `NotInitialized`
- `3` `ContractPaused`
- `4` `NotPaused`
- `5` `EmergencyActive`

## Protocol Governance

The escrow contract supports protocol parameter governance independent of pause controls:

- `initialize_governance(admin)`: One-time setup of governance admin
- `update_protocol_parameters(...)`: Update validation parameters
- `propose_governance_admin(new_admin)`: Propose admin transfer
- `accept_governance_admin()`: Accept admin transfer
- `get_governance_admin()`: Query current governance admin
- `get_pending_governance_admin()`: Query pending admin transfer
- `get_protocol_parameters()`: Query current parameters

### Governed Parameters

- `min_milestone_amount`: Minimum amount for any milestone (default: 1)
- `max_milestones`: Maximum milestones per contract (default: 16)
- `min_reputation_rating`: Minimum valid rating (default: 1)
- `max_reputation_rating`: Maximum valid rating (default: 5)

See [governance-security.md](./governance-security.md) for detailed governance security notes.

## Security Documentation

Comprehensive security documentation is available:

- **[threat-model.md](./threat-model.md)**: Complete threat analysis with 15 identified scenarios, attack vectors, mitigations, and residual risks
- **[governance-security.md](./governance-security.md)**: Protocol governance security model, operational procedures, and key management
- **[security.md](./security.md)**: Pause/emergency controls threat model and operational playbook
- **[performance-baselines.md](./performance-baselines.md)**: Gas and resource consumption baselines

## Security Notes

- Admin-only controls: pause and emergency operations require authenticated admin.
- Governance-only controls: parameter updates require authenticated governance admin.
- One-time initialization: both admin types cannot be replaced accidentally by repeated init calls.
- Emergency lock discipline: `unpause` is blocked while emergency mode is active.
- Fail-closed behavior: guarded functions revert whenever `paused == true`.
- Two-step admin transfer: governance admin transfer requires proposal and acceptance.
- Parameter validation: all protocol parameters validated before application.

## Operational Playbook

### Emergency Response

1. Detect incident and call `activate_emergency_pause`.
2. Investigate and remediate root cause.
3. Validate mitigations in test/staging.
4. Call `resolve_emergency` to restore service.
5. Publish incident summary for ecosystem transparency.

### Governance Parameter Update

1. Document proposed changes and rationale.
2. Allow community review period.
3. Call `update_protocol_parameters()` with new values.
4. Verify changes via `get_protocol_parameters()`.
5. Monitor contract creation patterns.
6. Announce changes to users.

### Governance Admin Transfer

1. Verify new admin identity through multiple channels.
2. Current admin calls `propose_governance_admin()`.
3. New admin calls `accept_governance_admin()`.
4. Verify transfer via `get_governance_admin()`.
5. Update documentation and announce transfer.
