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
set). The gate runs **before** any state read, TTL bump, or auth call so a
paused contract does not consume an auth cycle:

- `create_contract`
- `deposit_funds`
- `release_milestone`
- `refund_unreleased_milestones`
- `issue_reputation`
- `cancel_contract`

The same gate is already wired into `finalize_contract` and the two migration
endpoints (`propose_client_migration`, `accept_client_migration`), so the
contract surface is consistently gated.

Read-only queries (`get_contract`, `get_reputation`, `get_pending_reputation_credits`,
`get_admin`, `get_mainnet_readiness_info`, `is_paused`, `is_emergency`,
`get_finalization_record`, `get_milestone_approvals`) are never blocked.

Approval bookkeeping (`approve_milestone_release`) is mutation-adjacent; once
the broader approval-entry hardening issue lands, that entrypoint will be
added to this matrix.

## Read-only queries

The following queries are intentionally NOT gated and remain available even
during a pause or emergency so that off-chain dashboards, indexers, and the
admin can inspect state:

| Function | Purpose |
|---|---|
| `get_contract(id)` | Read contract participants and metadata |
| `get_milestones(id)` | Read milestone schedule |
| `get_refundable_balance(id)` | Read refundable balance |
| `get_milestone_approvals(id, idx)` | Read pending approvals (subject to TTL) |
| `get_reputation(addr)` | Read reputation record |
| `get_pending_reputation_credits(addr)` | Read pending credits |
| `get_finalization_record(id)` | Read immutable close metadata |
| `get_admin()` | Read the stored admin |
| `get_mainnet_readiness_info()` | Read readiness checklist |
| `is_paused()` / `is_emergency()` | Read pause/emergency flags |

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
- Every Blocked Operation runs `Self::require_not_paused(&env)` at the **very
  top** of its entrypoint — BEFORE auth, state load, TTL bumps, validation,
  mutation, and event publish. This guarantees:
  - A paused contract never burns an auth cycle.
  - Degenerate inputs cannot be used to drift past the gate inside a paused state.
  - Read-only queries are untouched.

## Tests

The pause/emergency gate is covered exhaustively in
`contracts/escrow/src/test/pause_controls.rs`. For every Blocked Operation the
suite verifies all four states:

1. **Ungated happy path** (e.g. `unpause_restores_*` / `resolve_emergency_restores_*`).
2. **Paused-only** (e.g. `pause_blocks_*`) — expect `ContractPaused`.
3. **Emergency-only** (e.g. `emergency_blocks_*`) — expect `EmergencyActive`.
4. **Auth-cycle safety** (`pause_gate_runs_before_auth_on_*`) — calling with
   the wrong caller surfaces `ContractPaused` and not `UnauthorizedRole`,
   proving the gate fires before `require_auth`.

Read-only queries are explicitly verified to remain reachable during both
`Paused` and `Emergency` states.
