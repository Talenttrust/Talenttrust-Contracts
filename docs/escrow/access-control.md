# Escrow Access Control Enforcement

This document describes role checks enforced in
`contracts/escrow/src/lib.rs` and the internal helper that implements them.

---

## `load_and_auth_admin` Helper

**Introduced in:** issue #337 — _Refactor repeated admin-load boilerplate into a
single helper_

All four admin-gated control entrypoints previously duplicated the same
three-step pattern:

```rust
let admin: Address = env
    .storage()
    .persistent()
    .get(&DataKey::Admin)
    .unwrap_or_else(|| env.panic_with_error(EscrowError::NotInitialized));
admin.require_auth();
```

This is now centralised in a single private function:

```rust
/// Load the stored admin address, panic with `NotInitialized` if absent,
/// and call `require_auth()` so that the Soroban auth engine records the
/// authorization requirement.  Returns the authenticated admin `Address`.
///
/// # Panics
/// - `NotInitialized` – no admin has been stored yet (i.e., `initialize`
///   was never called or the storage entry is missing).
/// - Soroban auth failure – the admin's signature is not present in the
///   current invocation's authorization context.
fn load_and_auth_admin(env: &Env) -> Address
```

### Security Properties

| Property | Guarantee |
|---|---|
| **Fail-closed** | Panics `NotInitialized` if storage is empty — no silent no-op |
| **Auth is required** | `require_auth()` is called _before_ any state mutation |
| **No privilege escalation** | Returns the stored address; callers cannot inject a different one |
| **Single source of truth** | All four entrypoints share identical semantics — no divergence risk |

### Dead-code elimination

The previous `require_admin(env, caller)` helper compared `caller` against the
stored admin but was **never called**. It has been removed to eliminate dead
code. `load_and_auth_admin` supersedes it.

---

## Implemented Checks

- `initialize(admin)` requires `admin.require_auth()` and can run only once.
- `pause`, `unpause`, `activate_emergency_pause`, and `resolve_emergency` all
  delegate to `load_and_auth_admin(&env)` which loads and authenticates the
  stored admin in one step.
- `create_contract(client, freelancer, milestone_amounts, deposit_mode)`
  requires `client.require_auth()`.
- `issue_reputation(contract_id, caller, freelancer, rating)` requires
  `caller.require_auth()` and `caller == contract.client`.
- `cancel_contract(contract_id, caller)` requires `caller.require_auth()` and
  the caller must be the stored client or freelancer.

---

## Current Release Caveat

`release_milestone(contract_id, milestone_index)` does not authenticate a
caller in the current implementation. It enforces pause state, contract
existence, milestone bounds, duplicate-release prevention, and available funded
balance only.

## Not Implemented

Approval-based release authorization, arbiter release authorization, and
multi-party release modes are not live entrypoints. Treat any approval or
arbiter release design as planned until implemented and tested.
