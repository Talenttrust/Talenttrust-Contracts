# Disputes Data Model & Invariants

## Overview

The disputes module allows contract parties (client and freelancer) to escalate
disagreements to an assigned arbiter. The arbiter resolves the dispute by
distributing the remaining escrowed balance according to one of four resolution
types.  All dispute logic lives in two files:

| File | Role |
| --- | --- |
| `contracts/escrow/src/dispute.rs` | Pure arithmetic helpers — no storage access |
| `contracts/escrow/src/lib.rs` | Root entrypoints that own auth, storage, events |

---

## 1. Data Model

All dispute types are defined in `contracts/escrow/src/types.rs`.

### 1.1 `DisputeResolution` enum (`types.rs:338-344`)

```rust
pub enum DisputeResolution {
    FullRefund,                    // code = 0
    PartialRefund,                 // code = 1
    FullPayout,                    // code = 2
    Split(DisputeSplit),           // code = 3
}
```

Each variant maps to a `u32` code via `DisputeResolution::code()` (`types.rs:347-354`)
for event serialisation.

### 1.2 `DisputeSplit` struct (`types.rs:329-333`)

```rust
pub struct DisputeSplit {
    pub client_amount: i128,
    pub freelancer_amount: i128,
}
```

### 1.3 Relevant `Contract` fields (`types.rs:215-226`)

```rust
pub struct Contract {
    pub arbiter: Option<Address>,   // must be Some to raise/resolve
    pub status: ContractStatus,     // Disputed while open
    pub funded_amount: i128,       // total ever deposited
    pub released_amount: i128,     // total paid to freelancer
    pub refunded_amount: i128,     // total returned to client
    // … other fields (client, freelancer, etc.)
}
```

### 1.4 Storage layout

There is **no dedicated dispute storage key**. All dispute state is stored
inline in `DataKey::Contract(contract_id)`:

| Key | Value | Written by |
| --- | --- | --- |
| `DataKey::Contract(contract_id)` | `Contract { arbiter, status, funded_amount, released_amount, refunded_amount, … }` | `raise_dispute` (`lib.rs:2217-2219`), `resolve_dispute` (`lib.rs:2310-2312`) |

---

## 2. State Machine

```
Funded ──→ Disputed ──→ Completed
    ↑                        |
PartiallyFunded ──→ Disputed ──→ Refunded
```

| Transition | Entrypoint | Guard (`lib.rs`) |
| --- | --- | --- |
| `Funded` → `Disputed` | `raise_dispute` | L2187-2214: init + not paused + caller is party + arbiter is Some + status is Funded or PartiallyFunded + not finalized |
| `PartiallyFunded` → `Disputed` | `raise_dispute` | Same as above |
| `Disputed` → `Completed` | `resolve_dispute` | L2271-2293: init + not paused + caller == assigned arbiter + status == Disputed + not finalized |
| `Disputed` → `Refunded` | `resolve_dispute` | Same as above; status determined by `final_status_after_resolution` returning `Refunded` iff `refunded_amount == funded_amount` |

---

## 3. Invariants

### 3.1 Core accounting invariant

After any dispute resolution:

```
released_amount + refunded_amount == funded_amount
```

**Enforcement:**
- `resolution_payouts` (`dispute.rs:34-41`) computes `available = funded_amount - released_amount - refunded_amount` using checked subtraction, panicking with `AccountingInvariantViolated` if the result would be negative (corrupted state).
- `resolve_dispute` (`lib.rs:2301-2302`) atomically updates both accumulators:
  ```rust
  contract.refunded_amount += client_payout;
  contract.released_amount += freelancer_payout;
  ```
  Because `client_payout + freelancer_payout == available`, the invariant is restored by construction.

### 3.2 Available balance positivity

```
available = funded_amount - released_amount - refunded_amount
available >= 0
```

Enforced via checked subtraction at `dispute.rs:34-41`.  A negative `available`
indicates corrupted accounting state and is unrecoverable — the contract panics.

### 3.3 Split conservation invariant

For `DisputeResolution::Split(split)`:

```
split.client_amount >= 0
split.freelancer_amount >= 0
split.client_amount + split.freelancer_amount == available
```

Enforced at `dispute.rs:55-66`:
1. Negative leg → `InvalidDisputeSplit`
2. Individual leg > available → `InvalidDisputeSplit` (Issue #572)
3. `safe_add_amounts` overflow → `PotentialOverflow`
4. Sum != available → `InvalidDisputeSplit`

### 3.4 Overflow protection

All arithmetic in `resolution_payouts` uses Soroban `checked_*` methods:
- `checked_sub` at line 36-37
- `checked_mul` at line 48
- `checked_div` at line 49
- `safe_add_amounts` (checked add wrapper) at line 62

### 3.5 Access control invariants

| Invariant | Enforced at |
| --- | --- |
| Only `contract.client` or `contract.freelancer` may raise | `lib.rs:2201-2203` → `UnauthorizedRole` |
| Only `contract.arbiter` may resolve | `lib.rs:2290-2293` → `UnauthorizedRole` |
| Arbiter must be `Some` to raise | `lib.rs:2206-2208` → `ArbiterRequired` |
| Contract must not be paused/emergency'd | `lib.rs:2188` (`require_not_paused`) |
| Contract must be initialized | `lib.rs:2187` (`require_initialized`) |
| Contract must not be finalized | `lib.rs:2198, 2282` (`require_not_finalized`) |

### 3.6 State-guard invariants

| Invariant | Enforced at |
| --- | --- |
| `raise_dispute` only from `Funded` or `PartiallyFunded` | `lib.rs:2211-2214` → `InvalidState` |
| `resolve_dispute` only from `Disputed` | `lib.rs:2285-2287` → `InvalidStatusTransition` |
| Single-use: second resolve fails | `lib.rs:2285-2287` (status no longer `Disputed`) |
| Raise after terminal state (`Completed`, `Refunded`, `Cancelled`) fails | `lib.rs:2211-2214` → `AlreadyFinalized` (via `require_not_finalized`) |

### 3.7 Final status determination

`dispute.rs:76-82` returns `Refunded` **iff** `contract.refunded_amount == contract.funded_amount`, meaning every stroop was returned to the client.  Otherwise it returns `Completed`.

`resolve_dispute` additionally grants a reputation credit to the freelancer when the final status is `Completed` (`lib.rs:2306-2308`).

---

## 4. Entrypoints

### 4.1 `raise_dispute(env, contract_id, caller)` — `lib.rs:2184-2229`

```
Parameters:   env: Env, contract_id: u32, caller: Address
Returns:      bool (always true on success; panics on failure)
Events:       ("dispute", "opened") with payload (contract_id, caller)
Storage:      DataKey::Contract(contract_id).status := Disputed
```

**Pre-flight checks (in order):**
1. `require_initialized` (L2187)
2. `require_not_paused` (L2188)
3. `caller.require_auth()` (L2189)
4. Load `Contract` from storage; panic `ContractNotFound` if absent (L2191-2195)
5. `require_not_finalized` (L2198)
6. `caller ∈ {contract.client, contract.freelancer}` else `UnauthorizedRole` (L2200-2203)
7. `contract.arbiter.is_some()` else `ArbiterRequired` (L2206-2208)
8. `contract.status ∈ {Funded, PartiallyFunded}` else `InvalidState` (L2211-2214)

**Mutation:**
- Sets `contract.status = Disputed` (L2216)
- Writes updated contract back to `DataKey::Contract(contract_id)` (L2217-2219)
- Extends TTL for the contract key (L2197, L2221)
- Publishes `("dispute", "opened")` event (L2223-2226)

### 4.2 `resolve_dispute(env, contract_id, arbiter, resolution)` — `lib.rs:2263-2322`

```
Parameters:   env: Env, contract_id: u32, arbiter: Address, resolution: DisputeResolution
Returns:      bool (always true on success; panics on failure)
Events:       ("dispute", "resolved") with payload (contract_id, resolution.code())
Storage:      DataKey::Contract(contract_id) — updated released/refunded amounts and status
```

**Pre-flight checks (in order):**
1. `require_initialized` (L2271)
2. `require_not_paused` (L2272)
3. `arbiter.require_auth()` (L2273)
4. Load `Contract` from storage; panic `ContractNotFound` if absent (L2275-2279)
5. `require_not_finalized` (L2282)
6. `contract.status == Disputed` else `InvalidStatusTransition` (L2285-2287)
7. `caller == contract.arbiter` else `UnauthorizedRole` (L2290-2293)

**Resolution:**
- Calls `dispute::resolution_payouts(&contract, &resolution)` (L2296-2298) — pure arithmetic, see §4.3
- Atomically updates `contract.refunded_amount += client_payout`, `contract.released_amount += freelancer_payout` (L2301-2302)
- Calls `dispute::final_status_after_resolution(&contract)` (L2305) — returns `Refunded` or `Completed`
- Grants reputation credit to freelancer if `Completed` (L2306-2308)
- Writes updated contract back to `DataKey::Contract(contract_id)` (L2310-2312)
- Extends TTL for the contract key (L2281, L2314)
- Publishes `("dispute", "resolved")` event (L2316-2319)

### 4.3 `resolution_payouts(contract, resolution)` — `dispute.rs:30-70`

Pure function. Returns `Result<(client_payout, freelancer_payout), Error>`.

| Variant | client_payout | freelancer_payout | Error conditions |
| --- | --- | --- | --- |
| `FullRefund` | `available` | `0` | `AccountingInvariantViolated` if `available < 0` |
| `FullPayout` | `0` | `available` | same |
| `PartialRefund` | `available - ⌊available×30/100⌋` | `⌊available×30/100⌋` | `PotentialOverflow` on intermediate mul/div |
| `Split(a, b)` | `a` | `b` | `InvalidDisputeSplit` if `a<0 \| b<0 \| a>available \| b>available \| a+b!=available`; `PotentialOverflow` on sum |

### 4.4 `final_status_after_resolution(contract)` — `dispute.rs:76-82`

```rust
pub fn final_status_after_resolution(contract: &Contract) -> ContractStatus {
    if contract.refunded_amount == contract.funded_amount {
        ContractStatus::Refunded
    } else {
        ContractStatus::Completed
    }
}
```

Called by `resolve_dispute` **after** `refunded_amount` and `released_amount` have been updated.

---

## 5. Worked Example

### Scenario: 3-milestone contract, dispute raised after first release, resolved via Split

**Contract creation** (`create_contract`):
```
client = Alice
freelancer = Bob
arbiter = Carol
milestones = [500, 300, 200]
total = 1000
status = Created
```

**Deposit** (`deposit_funds`):
```
funded_amount = 1000
released_amount = 0
refunded_amount = 0
status = Funded
available = 1000          // 1000 - 0 - 0
```

**Release milestone 0** (`release_milestone`):
```
released_amount = 500
available = 500            // 1000 - 500 - 0
```

**Alice raises dispute** (`raise_dispute` at `lib.rs:2184`):
1. `require_initialized` ✓
2. `require_not_paused` ✓
3. `Alice.require_auth()` ✓
4. Load contract ✓ (exists, id = 1)
5. `require_not_finalized` ✓
6. Alice == client ✓
7. `arbiter = Some(Carol)` ✓
8. `status == Funded` ✓
→ **state:** Disputed

**Carol resolves with Split(300, 200)** (`resolve_dispute` at `lib.rs:2263`):
1. Guards pass ✓
2. `resolution_payouts` called:
   - `available = 1000 - 500 - 0 = 500` ✓
   - `Split(300, 200)`: both non-negative, 300 ≤ 500, 200 ≤ 500, 300+200 = 500 ✓
   - returns `(300, 200)` ✓
3. `refunded_amount += 300` → 300
4. `released_amount += 200` → 700
5. `final_status_after_resolution`: refunded(300) != funded(1000) → `Completed` ✓
6. Reputation credit granted to Bob ✓
7. Event: `("dispute", "resolved", (1, 3))` (code 3 = Split) ✓

**Final state:**
```
funded_amount    = 1000
released_amount  = 700  (500 + 200)
refunded_amount  = 300
status           = Completed
——————————————
Invariant check:
  released(700) + refunded(300) = 1000 == funded(1000) ✓
```

### Scenario: PartialRefund after full deposit, no releases

**State before dispute:**
```
funded_amount = 1000, released_amount = 0, refunded_amount = 0
available = 1000
```

**Carol resolves PartialRefund:**
1. `resolution_payouts`: freelancer = floor(1000 × 30 / 100) = 300, client = 700
2. `refunded_amount = 700`, `released_amount = 300`
3. `final_status_after_resolution`: refunded(700) != funded(1000) → `Completed`
4. Reputation credit granted to Bob

---

## 6. Error Reference

Errors returned by the dispute entrypoints and their helpers:

| Error | Code | Raised by | Condition |
| --- | --- | --- | --- |
| `UnauthorizedRole` | 11 | `raise_dispute` | Caller not client/freelancer |
| `UnauthorizedRole` | 11 | `resolve_dispute` | Caller != assigned arbiter |
| `ArbiterRequired` | 42 | `raise_dispute` | `contract.arbiter` is `None` |
| `InvalidState` | 16 | `raise_dispute` | Status not Funded/PartiallyFunded |
| `InvalidStatusTransition` | 41 | `resolve_dispute` | Status != Disputed |
| `InvalidDisputeSplit` | 43 | `resolution_payouts` | Split amounts invalid |
| `AccountingInvariantViolated` | 44 | `resolution_payouts` | Available balance negative |
| `PotentialOverflow` | 45 | `resolution_payouts` | Arithmetic overflow |
| `ContractNotFound` | 10 | Both | Contract ID not in storage |
| `ContractPaused` | 37 | Both | Contract is paused |
| `AlreadyFinalized` | 46 | Both | Contract has finalization record |

---

## 7. Event Schema

| Event topic | Payload | When |
| --- | --- | --- |
| `("dispute", "opened")` | `(contract_id: u32, caller: Address)` | After `raise_dispute` status write |
| `("dispute", "resolved")` | `(contract_id: u32, resolution_code: u32)` | After `resolve_dispute` status write |

Resolution codes match the `DisputeResolution::code()` method:
- 0 = FullRefund
- 1 = PartialRefund
- 2 = FullPayout
- 3 = Split

---

## 8. Code Map

| Concern | File & lines |
| --- | --- |
| Dispute type definitions | `contracts/escrow/src/types.rs:328-355` |
| Pure payout arithmetic | `contracts/escrow/src/dispute.rs:30-70` |
| Final status determination | `contracts/escrow/src/dispute.rs:76-82` |
| `raise_dispute` entrypoint | `contracts/escrow/src/lib.rs:2184-2229` |
| `resolve_dispute` entrypoint | `contracts/escrow/src/lib.rs:2263-2322` |
| Internal guards (`require_*`) | `contracts/escrow/src/lib.rs:2133-2149` |
| Property-based payout tests | `contracts/escrow/src/test/resolution_payouts_prop.rs` |
| Unit & integration tests | `contracts/escrow/src/test/dispute.rs` |

---

## 9. Related Documentation

- [`docs/escrow/disputes.md`](./escrow/disputes.md) — Detailed dispute lifecycle, FAQ, integration examples
- [`docs/escrow/dispute-conservation-invariant.md`](./escrow/dispute-conservation-invariant.md) — Formal invariant proof with prior-release examples
- [`docs/escrow/status-transition-guardrails.md`](./escrow/status-transition-guardrails.md) — Full contract state machine
- [`docs/escrow/contract.md`](./escrow/contract.md) — Contract struct and storage layout
- [`docs/escrow/abi-reference.md`](./escrow/abi-reference.md) — All public entrypoint signatures

---

**Note:** This issue (#798) is purely documentation. No new code or tests were
added because the disputes module is already comprehensively tested (see
`contracts/escrow/src/test/dispute.rs` with 35+ tests and
`contracts/escrow/src/test/resolution_payouts_prop.rs` for property-based
coverage). The existing test suite covers all resolution variants, access
control, state-machine guards, accounting invariants, and overflow edge cases.