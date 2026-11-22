# Escrow Governance Security

The live escrow contract (`contracts/escrow/src/lib.rs`) is governed by a single
admin address initialized once via `initialize(admin)`. The admin can delegate
specific operational roles (`Pauser`, `Resolver`), hand off control via a
two-step admin transfer, and exercise privileged controls bounded by on-chain
caps and timelocks.

## Initialization and Admin Queries

- `initialize(admin: Address) -> Result<(), EscrowError>` — one-time setup;
  requires `admin.require_auth()` and fails with `AlreadyInitialized` if already
  set. Emits `("init",) -> admin`.
- `get_admin() -> Result<Address, EscrowError>` — returns the current admin
  address (or `NotInitialized`).
- `get_pending_admin() -> Option<Address>` — returns the pending admin nominee,
  if any.
- `version() -> u32` — on-chain code version (`1` until the first
  `execute_upgrade`, then incremented on each upgrade).

## Two-Step Admin Transfer

Admin handoff is split into nomination and acceptance so a typo'd address cannot
irrevocably lock governance:

- `propose_admin(new_admin: Address) -> Result<(), EscrowError>` — current admin
  only. Rejects nominating the current admin with `InvalidAdmin`. Overwrites any
  existing pending nomination. Emits `("adm_prop",) -> (current, new_admin)`.
- `accept_admin() -> Result<(), EscrowError>` — requires the pending nominee's
  authorization. Fails with `NoPendingAdmin` if no nomination is pending. Moves
  `Admin` to the nominee, clears `PendingAdmin`, and emits
  `("adm_acc",) -> (old, pending)`.
- `cancel_admin_proposal() -> Result<(), EscrowError>` — current admin only.
  Clears the pending nomination or fails with `NoPendingAdmin` if none is
  pending. Emits `("adm_cncl",) -> pending`.

All three rotation entrypoints (`propose_admin`, `accept_admin`,
`cancel_admin_proposal`) are intentionally callable while the contract is paused
so key rotation remains available as a recovery action. Because `PendingAdmin`
is cleared on acceptance or cancellation, a consumed or cancelled nomination
cannot be replayed (`NoPendingAdmin`).

## Role-Based Access Control

Operational duties can be delegated without sharing the admin key:

- `Role::Pauser` — authorized to call `set_paused` and `set_pause_flags`.
- `Role::Resolver` — authorized to call `resolve_dispute` (for disputes below
  the panel threshold and where the caller is not a party to the escrow).

Role management entrypoints:

- `grant_role(role: Role, account: Address) -> Result<(), EscrowError>` — admin
  only. Fails with `RoleUnchanged` if `account` already holds `role`. Emits
  `("role_set", role) -> (account, true)`.
- `revoke_role(role: Role, account: Address) -> Result<(), EscrowError>` — admin
  only. Fails with `RoleUnchanged` if `account` does not hold `role`. Emits
  `("role_set", role) -> (account, false)`.
- `has_role(role: Role, account: Address) -> bool` — returns `true` if `account`
  is the current admin or has been explicitly granted `role`.

**Implicit admin privileges:** The current admin implicitly holds every `Role`
(`has_role` returns `true` for the admin regardless of explicit role storage).
Rotating the admin via `accept_admin` automatically transfers these implicit
permissions to the new admin, while explicitly granted role memberships remain
bound to their respective accounts until revoked.

## Admin Powers, Bounds, and Risks

All mutating admin controls require the stored admin's Soroban authorization
(except `set_paused`/`set_pause_flags`, which also accept `Role::Pauser`, and
`resolve_dispute`, which also accepts `Role::Resolver`).

### 1. Pause (Circuit Breaker)
- **Entrypoints:** `set_paused(caller, paused)`, `set_pause_flags(caller, flags)`,
  `is_paused()`, `get_pause_flags()`, `get_paused_at()`.
- **Events / Errors:** `("paused",) -> paused` (`set_paused`),
  `("pause",) -> flags` (`set_pause_flags`); `Unauthorized` if caller lacks
  `Role::Pauser`, `InvalidAmount` if `flags` sets bits outside `PAUSE_ALL`.
- **Bounds & Risks:** A compromised admin or `Pauser` can halt operations across
  some or all categories (`PAUSE_CREATE`, `PAUSE_DEPOSIT`, `PAUSE_RELEASE`,
  `PAUSE_DISPUTE`, `PAUSE_CANCEL`, `PAUSE_REPUTATION`). Prolonged pausing is
  mitigated by `emergency_withdraw`: after `EMERGENCY_EXIT_DELAY_SECS` (30 days)
  of continuous pause, parties can unwind eligible escrows without admin
  cooperation.

### 2. Freeze (Per-Escrow)
- **Entrypoints:** `freeze_escrow(id, reason)`, `unfreeze_escrow(id)`,
  `is_frozen(id)`.
- **Events / Errors:** `("frozen", id) -> reason`, `("unfrozen", id) -> ()`;
  `ContractNotFound`, `EscrowFrozen` (already frozen), `InvalidStatus` (not
  frozen on unfreeze).
- **Bounds & Risks:** Freezing blocks all party-initiated mutations on that
  escrow with `EscrowFrozen`, while `resolve_dispute` remains available. A
  malicious admin could indefinitely stall a specific escrow.

### 3. Protocol Fee
- **Entrypoints:** `set_fee(fee_bps, treasury)`, `get_fee_config()`,
  `get_escrow_fee(id)`.
- **Events / Errors:** `("fee_set",) -> (fee_bps, treasury)`; `FeeTooHigh` if
  `fee_bps > MAX_FEE_BPS`, `InvalidParties` if `treasury` is the contract
  address.
- **Bounds & Risks:** Fees are hard-capped at `MAX_FEE_BPS = 500` (5%). Existing
  escrows snapshot their fee rate at creation, so fee increases only affect new
  escrows. However, the admin can redirect `treasury` or raise fees up to the
  5% cap for future escrows without a timelock.

### 4. Arbiter Panel
- **Entrypoints:** `set_panel(members, threshold)`, `get_panel()`.
- **Events / Errors:** `("panel",) -> (len, threshold)`; `InvalidPanel` if
  `threshold == 0`, `threshold > len`, `len > PANEL_MAX_MEMBERS`, or duplicate
  members are supplied.
- **Bounds & Risks:** Replacing the panel is allowed mid-dispute; from that
  point on, only votes from the new members count toward `threshold`. A
  compromised admin could swap the panel to influence high-value disputes.

### 5. Contract Upgrade (Timelocked)
- **Entrypoints:** `schedule_upgrade(wasm_hash)`, `cancel_upgrade()`,
  `execute_upgrade()`, `get_pending_upgrade()`.
- **Events / Errors:** `("upg_sched",) -> (wasm_hash, eta)`,
  `("upg_cncl",) -> wasm_hash`, `("upgraded",) -> (wasm_hash, new_version)`;
  `NoPendingUpgrade`, `UpgradeNotReady`, `ArithmeticOverflow`.
- **Bounds & Risks:** Scheduled WASM upgrades are subject to a 48-hour timelock
  (`UPGRADE_DELAY_SECS = 172_800`). Re-scheduling while an upgrade is pending
  resets the timer. `execute_upgrade` is intentionally allowed while paused so
  critical fixes can be deployed during an incident. Because an executed upgrade
  replaces contract code, admin key compromise carries full-contract takeover
  risk after the 48-hour window; monitoring `upg_sched` events is essential.

### 6. Surplus Sweep
- **Entrypoints:** `sweep_surplus(token, to)`, `get_liability(token)`.
- **Events / Errors:** `("sweep", token) -> (to, surplus)`; `NoSurplus` if the
  contract holds no untracked balance above tracked liabilities.
- **Bounds & Risks:** Sweeping is strictly bounded to
  `token.balance(contract) - liability_of(token)`, so the admin cannot withdraw
  funds backing active escrows.

### 7. Void Rating
- **Entrypoints:** `void_rating(id, reason)`.
- **Events / Errors:** `("rep_void", id) -> (freelancer, rating, reason)`;
  `RatingNotFound`, `RatingAlreadyVoided`.
- **Bounds & Risks:** Allows the admin to reverse a fraudulent rating's effect
  on reputation (blocked when `PAUSE_REPUTATION` is set). A compromised admin
  could selectively erase legitimate ratings.
