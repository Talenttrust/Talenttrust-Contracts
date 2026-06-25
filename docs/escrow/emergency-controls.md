# Emergency Controls — Escrow Contract

## Overview

The escrow contract supports admin-managed pause and emergency controls to block
all state-changing operations during incidents while preserving read access.

## Entrypoints

| Function | Description | Auth |
|---|---|---|
| `initialize(admin)` | One-time setup; sets the admin address | `admin.require_auth()` |
| `pause()` | Block all mutating operations | Admin |
| `unpause()` | Resume operations (fails if emergency is active) | Admin |
| `activate_emergency_pause()` | Set both `Paused` and `Emergency` flags | Admin |
| `resolve_emergency()` | Clear both flags and resume operations | Admin |
| `is_paused()` | Read-only flag query | None |
| `is_emergency()` | Read-only flag query | None |

## State Flags

Two boolean flags are stored under `DataKey::Paused` and `DataKey::Emergency` in
persistent storage.

| Flag | Set by | Cleared by |
|---|---|---|
| `Paused` | `pause()`, `activate_emergency_pause()` | `unpause()`, `resolve_emergency()` |
| `Emergency` | `activate_emergency_pause()` | `resolve_emergency()` |

`unpause()` is blocked while `Emergency` is active — only `resolve_emergency()`
can clear both flags together.

## Blocked Operations

When `Paused` is `true` or `Emergency` is `true`, the following entrypoints panic
with `ContractPaused` (or `EmergencyActive` when only the `Emergency` flag is
set):

- `create_contract`
- `deposit_funds`
- `release_milestone`
- `refund_unreleased_milestones`
- `issue_reputation`
- `cancel_contract`

Read-only queries (`get_contract`, `get_reputation`, `get_pending_reputation_credits`,
`is_paused`, `is_emergency`, `get_admin`, `get_mainnet_readiness_info`) are never blocked.

Approval bookkeeping (`approve_milestone_release`) is also mutation-adjacent;
once the broader approval-entry hardening issue lands, that entrypoint will be
added to this matrix.

## Events

| Event topic | Payload | Emitted by |
|---|---|---|
| `("paused", timestamp)` | `(admin,)` | `pause()` |
| `("unpaused", timestamp)` | `(admin,)` | `unpause()` |
| `("emergency", "activated")` | `(admin, timestamp)` | `activate_emergency_pause()` |
| `("emergency", "resolved")` | `(admin, timestamp)` | `resolve_emergency()` |

## Mainnet Readiness

`emergency_controls_enabled` in `MainnetReadinessInfo` is set to `true` after
the first call to `activate_emergency_pause()` or `resolve_emergency()`. This
confirms the emergency path has been exercised end-to-end before production use.

## Security Properties

- Only the stored admin address can toggle pause/emergency state.
- `initialize` can only be called once; a second call panics with `AlreadyInitialized`.
- `unpause` while emergency is active panics with `EmergencyActive` — emergency
  can only be cleared via `resolve_emergency`.
- All flag checks use `unwrap_or(false)` so an uninitialized contract defaults
  to unpaused (safe for fresh deployments before `initialize` is called).
