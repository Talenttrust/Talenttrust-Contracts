# Escrow Pause/Emergency Threat Model

## Scope

This model covers pause and emergency controls in `contracts/escrow/src/lib.rs`.

## Assumptions

- The admin key is securely managed.
- Soroban address authentication behaves as expected.
- Off-chain operators monitor incidents and invoke controls quickly.

## Threat Scenarios and Mitigations

1. Unauthorized pause/unpause/emergency calls.
Mitigation: `require_admin` gate with address auth on all control endpoints.

2. Re-initialization to seize control.
Mitigation: `initialize` is single-use and returns `AlreadyInitialized` on repeat calls.

3. Partial recovery from emergency state.
Mitigation: `unpause` returns `EmergencyActive` while emergency flag is set.

4. State-changing execution during incident containment.
Mitigation: all critical mutating endpoints check `ensure_not_paused`.

## Residual Risks

- Admin key compromise can still misuse pause controls.
- No timelock/multi-sig enforced in this contract version.
- Emergency actions are not event-logged in this baseline implementation.

## Recommended Next Hardening Steps

1. Move admin to a multi-sig account.
2. Add role separation for `pauser` and `resolver`.
3. Add on-chain event emission for pause state transitions.
4. Add optional time-delayed unpause for high-severity incidents.
