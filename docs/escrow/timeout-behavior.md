# Escrow timeout behavior

This document describes deadline-driven transitions and expiry boundary rules in the escrow contract.

## Scope

Applies to milestone approval and release in `contracts/escrow`.

## Rules

1. Every milestone includes `deadline_at` (unix timestamp in seconds).
2. `deadline_at` is initialized from contract creation time with a fixed timeout window.
3. Boundary handling is explicit:
   - `timestamp <= deadline_at`: action is allowed to proceed.
   - `timestamp > deadline_at`: milestone is expired.
4. On expired approval/release attempt:
   - contract status is transitioned to `Disputed`;
   - action is rejected with a timeout error.

## Security assumptions and threat scenarios

- **Timeout griefing resistance**: A participant cannot push state transitions after expiry; stale approvals/releases are rejected.
- **Deterministic boundary**: Inclusive boundary (`<=`) avoids ambiguous edge behavior at exact deadline.
- **Automatic dispute trigger**: Timeout transitions are stateful (`Funded -> Disputed`), preserving on-chain evidence of expiry-triggered contention.
- **Monotonic ledger time assumption**: Logic assumes ledger timestamp is monotonic and cannot be controlled by contract callers.

## Test coverage notes

Dedicated timeout tests validate:

- exact-deadline success (non-expired boundary),
- one-second-after-deadline failure,
- automatic `Disputed` transition on timeout-triggered rejection,
- release-path expiry behavior after a valid approval.
