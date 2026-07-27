# Settlement Storage Layout and TTL Policy

This document describes the persistent storage layout for settlement-related state
in the TalentTrust Escrow contract: the bound token address, the protocol fee
configuration, and the accumulated fee balance.

**Source of truth:** [`contracts/escrow/src/types.rs`](../../contracts/escrow/src/types.rs)
(DataKey enum), [`contracts/escrow/src/lib.rs`](../../contracts/escrow/src/lib.rs)
(read/write helpers and entrypoints),
[`contracts/escrow/src/governance.rs`](../../contracts/escrow/src/governance.rs)
(fee configuration).

**Related docs:** [`sac-custody.md`](./sac-custody.md) for the full custody model,
[`protocol-fees.md`](./protocol-fees.md) for the fee lifecycle,
[`state-persistence.md`](./state-persistence.md) for the full key map.

---

## Keys, Types, and Access Patterns

### `DataKey::SettlementToken`

| Property | Value |
|---|---|
| **Key** | `DataKey::SettlementToken` (bare enum variant, no payload) |
| **Type** | `Address` |
| **Storage class** | `persistent()` |
| **Written by** | `bind_settlement_token` — write-once, rejected after first bind |
| **Read by** | `deposit_funds`, `release_milestone`, `cancel_contract`, `refund_unreleased_milestones`, `withdraw_protocol_fees`, `get_settlement_token`, `is_settlement_token_bound` |
| **TTL bump on write** | None — default Soroban persistent TTL applies |
| **TTL bump on read** | None — key is never explicitly extended |

Reads go through a shared internal helper:

```rust
pub(crate) fn read_settlement_token(env: &Env) -> Option<Address> {
    env.storage().persistent().get(&DataKey::SettlementToken)
}
```

If the key is absent at call time (never bound or evicted), fund-moving
entrypoints panic with `Error::SettlementTokenNotConfigured`.

### `DataKey::ProtocolFeeBps`

| Property | Value |
|---|---|
| **Key** | `DataKey::ProtocolFeeBps` (bare enum variant) |
| **Type** | `u32` |
| **Storage class** | `persistent()` |
| **Written by** | `set_protocol_fee_bps`, `set_governed_params` |
| **Read by** | `get_protocol_fee_bps`, `read_protocol_fee_bps` (internal, used by `release_milestone`) |
| **TTL bump on write** | None |
| **TTL bump on read** | None |

Defaults to `0` (fee disabled) when unset. Must be ≤ 10 000 bps (100 %).

### `DataKey::AccumulatedProtocolFees`

| Property | Value |
|---|---|
| **Key** | `DataKey::AccumulatedProtocolFees` (bare enum variant) |
| **Type** | `i128` |
| **Storage class** | `persistent()` |
| **Written by** | `release_milestone` (incremented), `withdraw_protocol_fees` (decremented) |
| **Read by** | `get_accumulated_protocol_fees`, `release_milestone` (internal balance check) |
| **TTL bump on write** | `withdraw_protocol_fees` extends TTL; `release_milestone` does **not** |
| **TTL bump on read** | `get_accumulated_protocol_fees` does **not** extend TTL |

The only code path that explicitly bumps TTL for this key is
`withdraw_protocol_fees`:

```rust
env.storage().persistent().extend_ttl(
    &DataKey::AccumulatedProtocolFees,
    ttl::PERSISTENT_BUMP_THRESHOLD,   // 120 960 ledgers (~7 days)
    ttl::PERSISTENT_TTL_LEDGERS,      // 518 400 ledgers (~30 days)
);
```

The regular accrual path in `release_milestone` uses a bare `set`:

```rust
env.storage().persistent().set(
    &DataKey::AccumulatedProtocolFees,
    &(accumulated_fees + protocol_fee),
);
```

### `DataKey::Admin`

| Property | Value |
|---|---|
| **Key** | `DataKey::Admin` (bare enum variant) |
| **Type** | `Address` |
| **Storage class** | `persistent()` |
| **Written by** | `initialize`, `accept_governance_admin_impl` |
| **Read by** | All admin-gated entrypoints |
| **TTL bump on write** | None |
| **TTL bump on read** | None |

The admin address controls fee configuration, emergency controls, and fee
withdrawal. Admin rotation follows a two-step timelock pattern.

---

## TTL and Bump Strategy Summary

### Persistent keys without explicit TTL management

`SettlementToken`, `ProtocolFeeBps`, `NextContractId`, `Initialized`, `Paused`,
`Emergency`, `Admin`, `GovernedParameters`, and `ReadinessChecklist` are
written with `env.storage().persistent().set(...)` and **never** have their TTL
explicitly extended on read or write (except `NextContractId` which is extended
via `extend_next_contract_id_ttl`).

These keys depend on Soroban's default persistent-entry TTL. If the contract
goes unused for longer than that default TTL, these entries could be evicted,
making the contract inoperable until the admin rebinds them.

| Key | Bump on write | Bump on read |
|---|---|---|
| `SettlementToken` | — | — |
| `ProtocolFeeBps` | — | — |
| `AccumulatedProtocolFees` | Only in `withdraw_protocol_fees` | — |
| `Admin` | — | — |
| `GovernedParameters` | — | — |
| `ReadinessChecklist` | — | — |

### Persistent keys with explicit TTL management

`Contract(id)` and its paired milestone vector `(Contract(id), "milestones")` are
explicitly managed via `extend_contract_ttl` and `extend_milestone_ttl` (30-day
TTL, 7-day bump threshold). `NextContractId` is extended via
`extend_next_contract_id_ttl` on every `create_contract` call.

See [`ttl.rs`](../../contracts/escrow/src/ttl.rs) for the full set of TTL
constants and helpers.

### Transient keys

Approvals (`DataKey::MilestoneApprovals`) and client migrations
(`DataKey::PendingClientMigration`) live in `temporary()` storage with
fixed TTL — see [`storage-ttl.md`](./storage-ttl.md).

---

## Cross-Reference: Settlement Token Read Paths

Every entrypoint that moves funds reads the settlement token at call time:

| Entrypoint | How it reads | What happens if absent |
|---|---|---|
| `deposit_funds` | `read_settlement_token` → `unwrap_or_else(panic)` | `SettlementTokenNotConfigured` |
| `release_milestone` | `read_settlement_token` → `unwrap_or_else(panic)` | `SettlementTokenNotConfigured` |
| `refund_unreleased_milestones` | `read_settlement_token` → `unwrap_or_else(panic)` | `SettlementTokenNotConfigured` |
| `cancel_contract` | `read_settlement_token` → `unwrap_or_else(panic)` | Falls back to `NotInitialized` |
| `withdraw_protocol_fees` | `read_settlement_token` → `unwrap_or_else(panic)` | `SettlementTokenNotConfigured` |
| `get_settlement_token` | `read_settlement_token` (returns `Option`) | Returns `None` |
| `is_settlement_token_bound` | `read_settlement_token().is_some()` | Returns `false` |

---

## Eviction Risk and Remediation

Because `SettlementToken`, `ProtocolFeeBps`, and `AccumulatedProtocolFees` are
never explicitly TTL-bumped, they are at risk of eviction if the contract goes
unused for an extended period (Soroban's default persistent TTL). The
`AccumulatedProtocolFees` key is partially protected by the explicit bump in
`withdraw_protocol_fees`, but the accrual path in `release_milestone` does not
bump it.

A future improvement should add TTL extension to `read_settlement_token` and to
the `AccumulatedProtocolFees` write in `release_milestone`, matching the pattern
used by `withdraw_protocol_fees` and the contract/milestone helpers.
