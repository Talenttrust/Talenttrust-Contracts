# Protocol Fee Model

This document is the authoritative reference for the TalentTrust escrow protocol fee
lifecycle — from governance configuration through per-milestone accrual to treasury
withdrawal.

**Source of truth:** [`contracts/escrow/src/lib.rs`](../../contracts/escrow/src/lib.rs),
[`contracts/escrow/src/governance.rs`](../../contracts/escrow/src/governance.rs)

**Test coverage:** [`contracts/escrow/src/test/protocol_fees.rs`](../../contracts/escrow/src/test/protocol_fees.rs),
[`contracts/escrow/src/protocol_fees_test.rs`](../../contracts/escrow/src/protocol_fees_test.rs)

---

## Table of Contents

1. [Basis-Point Unit](#1-basis-point-unit)
2. [Fee Formula and Rounding](#2-fee-formula-and-rounding)
3. [Governance — Setting the Rate](#3-governance--setting-the-rate)
4. [Accrual — Where and When Fees Are Stored](#4-accrual--where-and-when-fees-are-stored)
5. [Treasury Withdrawal](#5-treasury-withdrawal)
6. [Worked Numeric Example](#6-worked-numeric-example)
7. [Sequence Diagram](#7-sequence-diagram)
8. [Entrypoint Cross-Reference](#8-entrypoint-cross-reference)
9. [Security Notes](#9-security-notes)
10. [Error Reference](#10-error-reference)

---

## 1. Basis-Point Unit

Fees are expressed in **basis points (bps)**, where:

```
10_000 bps = 100%
 1_000 bps = 10%
   500 bps = 5%
   250 bps = 2.5%
     1 bps = 0.01%
```

The maximum configurable fee rate is **10 000 bps (100%)**. Setting a value above
`10_000` is rejected with `InvalidProtocolParameters`. A value of `0` disables fee
collection entirely (the default after `initialize`).

---

## 2. Fee Formula and Rounding

```
protocol_fee = amount * fee_bps / 10_000   (integer floor division)
net_payout   = amount - protocol_fee
```

Implemented in `Escrow::calculate_protocol_fee` (`lib.rs`):

```rust
pub fn calculate_protocol_fee(env: &Env, amount: i128, fee_bps: u32) -> i128 {
    if fee_bps == 0 {
        return 0;
    }
    let product = amount
        .checked_mul(fee_bps as i128)
        .unwrap_or_else(|| env.panic_with_error(EscrowError::PotentialOverflow));
    product / 10_000
}
```

Key properties:

| Property | Behaviour |
|---|---|
| **Rounding** | Floor (integer division). The fee never rounds up. |
| **Zero-fee short-circuit** | Returns `0` immediately when `fee_bps == 0`. |
| **Overflow guard** | `checked_mul` panics with `PotentialOverflow` (#28) if `amount * fee_bps` exceeds `i128::MAX`. |
| **Fee cap** | `fee ≤ amount` always holds for `fee_bps ∈ [0, 10_000]` and `amount ≥ 0`. |
| **Sub-threshold** | When `amount * fee_bps < 10_000` the fee is `0` (e.g. 9 stroops at 10% → fee = 0). |

---

## 3. Governance — Setting the Rate

Two entrypoints set the fee rate. Both are **admin-gated** and require the stored admin
address (under `DataKey::Admin`) to authorize the call.

### `set_protocol_fee_bps(env, new_bps)`

Sets the fee rate in isolation. Rejects `new_bps > 10_000`. Emits:

```
Topics : (Symbol "protocol_fee_bps",)
Data   : (old_bps: u32, new_bps: u32, admin: Address, timestamp: u64)
```

### `set_governed_params(env, admin, protocol_fee_bps, max_escrow_total_stroops)`

Sets the fee rate alongside `max_escrow_total_stroops` in a single atomic call and
flips `ReadinessChecklist::governed_params_set` to `true`. Rejects
`protocol_fee_bps > 10_000`.

**Storage key:** `DataKey::ProtocolFeeBps` (persistent storage)

The rate takes effect **immediately** for the next `release_milestone` call; already
released milestones are unaffected.

---

## 4. Accrual — Where and When Fees Are Stored

### When fees accrue

A protocol fee is deducted on every successful call to **`release_milestone`**. No fee
is charged on deposits, refunds, or cancellations.

### How fees are computed at release

Inside `release_milestone` (`lib.rs`):

```rust
let fee_bps = Self::read_protocol_fee_bps(&env);
let protocol_fee: i128 = if fee_bps > 0 {
    Self::calculate_protocol_fee(&env, gross_amount, fee_bps)
} else {
    0
};
let net_amount = gross_amount - protocol_fee;
```

### Where fees are stored

Accumulated fees are stored in **persistent storage** under the key:

```rust
DataKey::AccumulatedProtocolFees   // type: i128
```

After each release the running total is incremented:

```rust
env.storage().persistent().set(
    &DataKey::AccumulatedProtocolFees,
    &(accumulated_fees + protocol_fee),
);
```

The fee amount **stays commingled** with the escrow's SAC token balance inside the
contract address. It is not moved to a separate account. The accounting invariant tracks
it separately:

```
released_amount + refunded_amount + accumulated_fees ≤ funded_amount
```

If this invariant is violated `release_milestone` panics with
`AccountingInvariantViolated` (#27).

### Reading the accumulated balance

```rust
// Public, auth-free read
Escrow::get_accumulated_protocol_fees(env) -> i128
```

---

## 5. Treasury Withdrawal

### Entrypoint

```rust
pub fn withdraw_protocol_fees(env: Env, amount: i128, to: Address) -> bool
```

| Check | Detail |
|---|---|
| **Initialization** | Requires `initialize` to have been called (`NotInitialized`). |
| **Pause gate** | Blocked while `DataKey::Paused` is `true` (`ContractPaused`). |
| **Authorization** | The stored admin (`DataKey::Admin`) must sign. |
| **Positive amount** | `amount > 0`, else `AmountMustBePositive` (#30). |
| **Sufficient balance** | `amount ≤ AccumulatedProtocolFees`, else `InsufficientAccumulatedFees` (#13). |
| **Token bound** | A settlement token must be bound, else `SettlementTokenNotConfigured`. |

### Execution

1. Decrements `DataKey::AccumulatedProtocolFees` by `amount`.
2. Calls `SAC::transfer(from: escrow_contract, to: treasury_address, amount)`.
3. Extends the TTL of `DataKey::AccumulatedProtocolFees`.
4. Emits:

```
Topics : (symbol_short!("fee"), symbol_short!("withdraw"))
Data   : (admin: Address, to: Address, amount: i128, timestamp: u64)
```

Partial withdrawals are supported — `amount` can be less than the full accumulated
balance.

---

## 6. Worked Numeric Example

**Setup:**
- Protocol fee rate: **250 bps (2.5%)**
- Contract milestones: `[10_000, 25_000, 33_333]` stroops

| Step | Milestone amount | Formula | Fee (floor) | Net to freelancer |
|------|-----------------|---------|-------------|-------------------|
| Release #0 | 10 000 | 10 000 × 250 / 10 000 | **250** | 9 750 |
| Release #1 | 25 000 | 25 000 × 250 / 10 000 | **625** | 24 375 |
| Release #2 | 33 333 | 33 333 × 250 / 10 000 = 8 333.25 → floor | **833** | 32 500 |

After all three releases:

```
AccumulatedProtocolFees = 250 + 625 + 833 = 1_708 stroops
```

**Withdrawal:**

The admin calls `withdraw_protocol_fees(amount: 1_708, to: treasury_address)`:

- `DataKey::AccumulatedProtocolFees` drops from `1_708` to `0`.
- The SAC contract transfers `1_708` stroops from the escrow address to `treasury_address`.

### Sub-threshold rounding example

With 100 bps (1%) and an amount of **9 stroops**:

```
9 × 100 = 900   →   900 / 10_000 = 0   (fee = 0, freelancer receives full 9)
```

Any milestone amount below **100 stroops at 1%** (equivalently, `amount < 10_000 / fee_bps`)
yields a zero fee due to integer floor division.

---

## 7. Sequence Diagram

```
Admin                       Escrow Contract              Freelancer          Treasury
  |                               |                           |                  |
  |-- set_protocol_fee_bps(250) ->|                           |                  |
  |<- true ----------------------|                           |                  |
  |                               |                           |                  |
Client                             |                           |                  |
  |-- deposit_funds(contract_id, 68_333) ------------------>|                  |
  |                               |                           |                  |
  |-- approve_milestone_release(0) ----------------------->|                  |
  |-- release_milestone(0) -------------------------------->|                  |
  |                               |-- calculate_protocol_fee(10_000, 250)       |
  |                               |   fee = 250; net = 9_750                   |
  |                               |-- SAC::transfer(escrow → freelancer, 9_750)-> 9_750
  |                               |-- AccumulatedProtocolFees += 250            |
  |                               |                           |                  |
  |-- approve_milestone_release(1) ----------------------->|                  |
  |-- release_milestone(1) -------------------------------->|                  |
  |                               |   fee = 625; net = 24_375                  |
  |                               |-- SAC::transfer(escrow → freelancer, 24_375)-> 24_375
  |                               |-- AccumulatedProtocolFees += 625            |
  |                               |                           |                  |
Admin                             |                           |                  |
  |-- withdraw_protocol_fees(875, treasury) -------------->|                  |
  |                               |-- verify admin auth                         |
  |                               |-- AccumulatedProtocolFees: 875 → 0          |
  |                               |-- SAC::transfer(escrow → treasury, 875) ---------> 875
  |                               |-- emit ("fee", "withdraw")                  |
  |<- true ----------------------|                           |                  |
```

---

## 8. Entrypoint Cross-Reference

| Entrypoint | File | Purpose |
|---|---|---|
| `set_protocol_fee_bps` | `governance.rs`, `lib.rs` | Set fee rate (admin-only) |
| `set_governed_params` | `governance.rs`, `lib.rs` | Set fee + cap atomically (admin-only) |
| `get_protocol_fee_bps` | `lib.rs` | Read current fee rate (public) |
| `calculate_protocol_fee` | `lib.rs` | Compute fee for a given amount (internal helper, `pub` for tests) |
| `read_protocol_fee_bps` | `lib.rs` | Internal read, used inside `release_milestone` |
| `release_milestone` | `lib.rs` | Accrues fee into `AccumulatedProtocolFees` on each release |
| `get_accumulated_protocol_fees` | `lib.rs` | Read accrued total (public) |
| `withdraw_protocol_fees` | `lib.rs` | Drain fees to treasury (admin-only) |

**Storage keys** (defined in `types.rs` → `DataKey`):

| Key | Type | Description |
|---|---|---|
| `DataKey::ProtocolFeeBps` | `u32` | Current fee rate in basis points |
| `DataKey::AccumulatedProtocolFees` | `i128` | Running total of accrued fees |
| `DataKey::Admin` | `Address` | Admin who can change the rate and withdraw |

---

## 9. Security Notes

### Admin-only withdrawal

`withdraw_protocol_fees` requires `admin.require_auth()` where `admin` is the address
stored under `DataKey::Admin`. No other address — including the contract itself, the
client, or the freelancer — can drain the accumulated fees.

Admin rotation follows the two-step timelock pattern
(`propose_governance_admin` → `accept_governance_admin` after
`ADMIN_ROTATION_MIN_DELAY_LEDGERS`). See
[`docs/escrow/governance-security.md`](./governance-security.md).

### Pause gate

`withdraw_protocol_fees` is blocked while `DataKey::Paused` is `true`. This prevents
fee drainage during an active emergency freeze, consistent with all other mutating
entrypoints.

### Fee cap

`protocol_fee_bps > 10_000` is always rejected, ensuring the fee can never exceed 100%
of the milestone amount. The accounting invariant
(`released_amount + refunded_amount + accumulated_fees ≤ funded_amount`) provides a
second layer of protection against over-collection.

### Commingled balance

Accumulated fees remain inside the escrow contract's SAC token balance alongside
undisbursed milestone funds. They are tracked separately in
`DataKey::AccumulatedProtocolFees`; the invariant check in `release_milestone` ensures
available balance calculations always exclude already-accumulated fees from the spendable
pool for future releases.

### Overflow protection

`calculate_protocol_fee` uses `checked_mul` and panics with `PotentialOverflow` (#28)
if `amount × fee_bps` would exceed `i128::MAX`. In practice, Soroban's
`MAX_SINGLE_AMOUNT_STROOPS` bound keeps operands well within safe range.

---

## 10. Error Reference

| Error | Code | Trigger |
|---|---|---|
| `InsufficientAccumulatedFees` | #13 | `withdraw_protocol_fees` called with `amount > AccumulatedProtocolFees` |
| `InvalidProtocolParameters` | — | `set_protocol_fee_bps` or `set_governed_params` called with `bps > 10_000` |
| `AmountMustBePositive` | #30 | `withdraw_protocol_fees` called with `amount ≤ 0` |
| `PotentialOverflow` | #28 | `calculate_protocol_fee` detects `amount × fee_bps` overflow |
| `AccountingInvariantViolated` | #27 | Post-release invariant check fails (should never occur in correct usage) |
| `ContractPaused` | #16 | `withdraw_protocol_fees` called while contract is paused |
| `UnauthorizedRole` | #15 | `withdraw_protocol_fees` or fee-setter called by non-admin |
| `NotInitialized` | #14 | Any fee entrypoint called before `initialize` |
| `SettlementTokenNotConfigured` | — | `withdraw_protocol_fees` called before `bind_settlement_token` |

---

*This document covers only the protocol fee subsystem. For the full custody model and
accounting invariant see [`sac-custody.md`](./sac-custody.md). For governance and admin
rotation see [`governance-security.md`](./governance-security.md).*
