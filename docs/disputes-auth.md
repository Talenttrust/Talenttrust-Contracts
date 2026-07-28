# Disputes authorization and access rules

This document describes **who may call** the dispute entrypoints, **in which
contract states**, and **which typed errors** reject unauthorized or invalid
calls. It is derived from the auth and state checks in
[`contracts/escrow/src/lib.rs`](../contracts/escrow/src/lib.rs)
(`raise_dispute`, `resolve_dispute`) and the shared gates in
[`contracts/escrow/src/finalize.rs`](../contracts/escrow/src/finalize.rs)
(`require_not_paused`, `require_not_finalized`).

Payout arithmetic lives in
[`contracts/escrow/src/dispute.rs`](../contracts/escrow/src/dispute.rs) and is
out of scope except where it produces auth-adjacent rejections
(`InvalidDisputeSplit`, `AccountingInvariantViolated`, `PotentialOverflow`).

---

## Roles

| Role | Stored where | Dispute powers |
| --- | --- | --- |
| **Client** | `Contract.client` | May call `raise_dispute` when the contract is disputable. Cannot resolve. |
| **Freelancer** | `Contract.freelancer` | May call `raise_dispute` when the contract is disputable. Cannot resolve. |
| **Arbiter** | `Contract.arbiter` (`Option<Address>`) | May call `resolve_dispute` only when equal to the assigned arbiter. Cannot raise. |
| **Anyone else** | — | Rejected with `UnauthorizedRole` on both entrypoints. |
| **Admin / pause controller** | `DataKey::Admin` | Does not participate in dispute calls directly; pause/emergency rails block both entrypoints for everyone. |

Notes:

- Client and freelancer are **mutually exclusive** parties for raising: either
  may open a dispute; neither can settle it.
- An arbiter must be assigned (`Some`) before `raise_dispute` succeeds. Contracts
  created with `arbiter: None` cannot enter the dispute path
  (`ArbiterRequired`).
- Soroban `require_auth()` runs on the **caller** (`raise_dispute`) or the
  **arbiter argument** (`resolve_dispute`) before role/state mutation checks
  complete.

---

## Shared gates (both entrypoints)

Both `raise_dispute` and `resolve_dispute` run these checks first:

| Order | Check | Rejection |
| --- | --- | --- |
| 1 | `require_initialized` — `DataKey::Initialized` is true | `NotInitialized` |
| 2 | `require_not_paused` — neither pause nor emergency is active | `ContractPaused` or `EmergencyActive` |
| 3 | Caller / arbiter `require_auth()` | Soroban auth failure (no contract error code) |

Then each entrypoint loads `DataKey::Contract(contract_id)` and continues:

| Check | Rejection |
| --- | --- |
| Contract storage present | `ContractNotFound` |
| `require_not_finalized(contract_id)` — no finalization record | `AlreadyFinalized` |

---

## `raise_dispute(env, contract_id, caller) -> bool`

**Source:** `Escrow::raise_dispute` in `lib.rs`.

### Allowed callers and states

| Caller | Allowed contract status | Outcome |
| --- | --- | --- |
| Client | `Funded` or `PartiallyFunded` | Status → `Disputed`; emits `("dispute", "opened")` |
| Freelancer | `Funded` or `PartiallyFunded` | Same |

### Rejection matrix

| Condition | Error |
| --- | --- |
| Shared gates fail | see table above |
| `caller` is neither client nor freelancer | `UnauthorizedRole` |
| `contract.arbiter` is `None` | `ArbiterRequired` |
| Status is not `Funded` / `PartiallyFunded` (e.g. `Created`, `Disputed`, `Completed`, `Refunded`, `Cancelled`) | `InvalidState` |

The assigned arbiter **cannot** raise a dispute unless they are also the
client or freelancer address (they normally are not).

### Allowed transition

```text
Funded | PartiallyFunded  --raise_dispute(party)-->  Disputed
```

---

## `resolve_dispute(env, contract_id, arbiter, resolution) -> bool`

**Source:** `Escrow::resolve_dispute` in `lib.rs`.

### Allowed callers and states

| Caller | Allowed contract status | Outcome |
| --- | --- | --- |
| Assigned arbiter only | `Disputed` | Applies payouts; status → `Completed` or `Refunded`; emits `("dispute", "resolved")` |

Final status selection is `final_status_after_resolution`: `Refunded` only when
`refunded_amount == funded_amount`, otherwise `Completed`.

### Rejection matrix

| Condition | Error |
| --- | --- |
| Shared gates fail | see table above |
| Status is not `Disputed` | `InvalidStatusTransition` |
| `arbiter` does not match `contract.arbiter` (including when arbiter is `None`) | `UnauthorizedRole` |
| Split legs negative, non-conserving, or exceed available | `InvalidDisputeSplit` |
| Available balance would be negative | `AccountingInvariantViolated` |
| Intermediate arithmetic overflows | `PotentialOverflow` |

Client and freelancer **cannot** resolve, even when authenticated.

### Allowed transitions

```text
Disputed  --resolve_dispute(arbiter, FullRefund)-->              Refunded   (typical full client refund)
Disputed  --resolve_dispute(arbiter, FullPayout|PartialRefund|Split)-->  Completed  (any freelancer credit or non-full refund)
```

Exact payouts depend on `resolution_payouts` and prior
`released_amount` / `refunded_amount`; see
[`docs/escrow/dispute-resolution.md`](escrow/dispute-resolution.md).

---

## Auth check order (reference)

### Raise

1. `require_initialized`
2. `require_not_paused`
3. `caller.require_auth()`
4. Load contract → `ContractNotFound`
5. TTL bump + `require_not_finalized`
6. Role: client **or** freelancer → else `UnauthorizedRole`
7. Arbiter present → else `ArbiterRequired`
8. Status ∈ {`Funded`, `PartiallyFunded`} → else `InvalidState`
9. Write `Disputed` + emit opened event

### Resolve

1. `require_initialized`
2. `require_not_paused`
3. `arbiter.require_auth()`
4. Load contract → `ContractNotFound`
5. TTL bump + `require_not_finalized`
6. Status == `Disputed` → else `InvalidStatusTransition`
7. `arbiter == contract.arbiter` → else `UnauthorizedRole`
8. `resolution_payouts` → typed math errors
9. Update accounting, final status, emit resolved event

---

## Worked example

Scenario: client `C` and freelancer `F` create contract `42` with arbiter `A`,
deposit until status is `Funded`, then escalate and settle.

```rust
// 1) Party opens the dispute — only C or F may call.
escrow.raise_dispute(&42u32, &C);
// OK: C.require_auth(), C == contract.client, arbiter is Some(A),
//     status was Funded → now Disputed.
// Event: ("dispute", "opened") with (42, C)

// Rejected alternatives at this step:
// escrow.raise_dispute(&42, &outsider);  // UnauthorizedRole
// escrow.raise_dispute(&42, &A);         // UnauthorizedRole (arbiter is not a party)
// escrow.raise_dispute(&42, &C);         // InvalidState if already Disputed / not funded
// // if arbiter was None at create time → ArbiterRequired

// 2) Only the assigned arbiter may settle.
escrow.resolve_dispute(&42u32, &A, &DisputeResolution::PartialRefund);
// OK: A.require_auth(), status Disputed, A == contract.arbiter.
// Accounting updated; status → Completed (freelancer received 30% floor).
// Event: ("dispute", "resolved") with (42, resolution code)

// Rejected alternatives at this step:
// escrow.resolve_dispute(&42, &C, &DisputeResolution::FullRefund); // UnauthorizedRole
// escrow.resolve_dispute(&42, &A, &DisputeResolution::FullRefund); // InvalidStatusTransition if not Disputed
// escrow.resolve_dispute(&42, &A, &DisputeResolution::Split(...)); // InvalidDisputeSplit if sum != available
```

Pause / emergency / finalization overlays (any role):

```rust
// While paused or emergency-active:
escrow.raise_dispute(&42, &C);    // ContractPaused or EmergencyActive
escrow.resolve_dispute(&42, &A, &DisputeResolution::FullPayout); // same

// After finalize_contract on a Disputed contract:
escrow.resolve_dispute(&42, &A, &DisputeResolution::FullRefund); // AlreadyFinalized
```

---

## Quick lookup

| Entrypoint | Who | From status | To status | Typical reject codes |
| --- | --- | --- | --- | --- |
| `raise_dispute` | client or freelancer | `Funded` / `PartiallyFunded` | `Disputed` | `UnauthorizedRole`, `ArbiterRequired`, `InvalidState`, `ContractPaused`, `EmergencyActive`, `AlreadyFinalized`, `NotInitialized`, `ContractNotFound` |
| `resolve_dispute` | assigned arbiter | `Disputed` | `Completed` / `Refunded` | `UnauthorizedRole`, `InvalidStatusTransition`, `InvalidDisputeSplit`, `AccountingInvariantViolated`, `PotentialOverflow`, plus shared gates |

For broader dispute product docs see [`docs/escrow/disputes.md`](escrow/disputes.md).
For the public ABI signatures see [`docs/escrow/abi-reference.md`](escrow/abi-reference.md).
