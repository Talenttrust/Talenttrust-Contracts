# Arbiter Authorization and Access Rules

This document describes every entrypoint in the TalentTrust escrow contract
that the arbiter role may or must interact with, together with the exact
authorization checks enforced in source. All rules are verified against
`contracts/escrow/src/lib.rs`, `contracts/escrow/src/approvals.rs`,
`contracts/escrow/src/finalize.rs`, and `contracts/escrow/src/create_contract.rs`.

---

## 1. Role Definitions

The escrow contract recognises four participant addresses:

| Role | Description |
|------|-------------|
| **Admin** | Contract deployer / governance key. Controls pause, emergency, protocol-fee, and admin-rotation. Has **no** role in individual escrow contracts. |
| **Client** | The party funding an escrow contract. Creates contracts and pays milestone deposits. |
| **Freelancer** | The party delivering work. Receives milestone payouts upon release. |
| **Arbiter** | An optional, independent third party stored per-contract in `Contract.arbiter: Option<Address>`. Participates in milestone approval, dispute raising, dispute resolution, and finalization depending on the `ReleaseAuthorization` mode. |

> **Arbiter is always optional at the contract level** — `Contract.arbiter` is an
> `Option<Address>`. However, specific `ReleaseAuthorization` modes
> (`ArbiterOnly`, `ClientAndArbiter`) **require** an arbiter to be provided at
> `create_contract` time or the call panics with `MissingArbiter`.

---

## 2. Contract States

The arbiter's rights are conditioned on `ContractStatus`. The full lifecycle:

```
Created → (Funded | PartiallyFunded) → Completed
                                    ↓
                                Disputed → (Completed | Refunded)
                                    ↑
Created → Cancelled
(Funded | PartiallyFunded) → Refunded
```

| State | Code | Description |
|-------|------|-------------|
| `Created` | 0 | Contract exists; no deposit received yet |
| `Accepted` | 1 | Reserved for future use |
| `Funded` | 2 | Full deposit received |
| `Completed` | 3 | All milestones released (or mix of released/refunded) |
| `Disputed` | 4 | Dispute opened; milestone releases blocked |
| `Cancelled` | 5 | Client cancelled before any release |
| `Refunded` | 6 | All milestones refunded |
| `PartiallyFunded` | 7 | Some deposit received; per-milestone allocation underway |

---

## 3. Arbiter Presence Rules at Contract Creation

**Entrypoint:** `create_contract` — [`create_contract.rs` L41–L174](../contracts/escrow/src/create_contract.rs)

```rust
// Validate arbiter requirement based on release authorization mode.
match release_authorization {
    ReleaseAuthorization::ArbiterOnly | ReleaseAuthorization::ClientAndArbiter
        if arbiter.is_none() =>
    {
        env.panic_with_error(EscrowError::MissingArbiter);
    }
    _ => {}
}

// Validate arbiter is distinct from both client and freelancer.
if let Some(ref arb) = arbiter {
    if arb == &client || arb == &freelancer {
        env.panic_with_error(EscrowError::InvalidArbiter);
    }
}
```

| `ReleaseAuthorization` | Arbiter required? | Error if absent |
|------------------------|-------------------|-----------------|
| `ClientOnly` | No | — |
| `ClientAndArbiter` | **Yes** | `MissingArbiter` |
| `ArbiterOnly` | **Yes** | `MissingArbiter` |
| `MultiSig` | No | — |

**Additional constraint (all modes):** If an arbiter address *is* supplied, it
must differ from both `client` and `freelancer`; otherwise the call panics with
`InvalidArbiter`.

---

## 4. Release Authorization Modes — Arbiter's Role

`ReleaseAuthorization` is set once at `create_contract` and stored immutably in
`Contract.release_authorization`. It governs two related operations:

- **`approve_milestone_release`** — who may record a pre-approval.
- **`release_milestone`** — who may trigger the token transfer.

### 4.1 Who May Approve (`approve_milestone_release` → `approvals::approve_milestone`)

Source: [`approvals.rs` L96–L117](../contracts/escrow/src/approvals.rs)

```rust
match contract.release_authorization {
    ReleaseAuthorization::ClientOnly => {
        if !is_client { return Err(Error::UnauthorizedRole); }
    }
    ReleaseAuthorization::ArbiterOnly => {
        if !is_arbiter { return Err(Error::UnauthorizedRole); }
    }
    ReleaseAuthorization::ClientAndArbiter => {
        if !is_client && !is_arbiter { return Err(Error::UnauthorizedRole); }
    }
    ReleaseAuthorization::MultiSig => {
        if !is_client && !is_freelancer { return Err(Error::UnauthorizedRole); }
    }
}
```

| Mode | Client | Freelancer | **Arbiter** |
|------|--------|------------|-------------|
| `ClientOnly` | ✅ | ❌ | ❌ |
| `ArbiterOnly` | ❌ | ❌ | ✅ |
| `ClientAndArbiter` | ✅ | ❌ | ✅ |
| `MultiSig` | ✅ | ✅ | ❌ |

**Required state for approval:** `ContractStatus::Funded` or `ContractStatus::PartiallyFunded`.

### 4.2 Who May Release (`release_milestone`)

Source: [`lib.rs` L722–L743](../contracts/escrow/src/lib.rs)

```rust
let is_arbiter = contract.arbiter.as_ref() == Some(&caller);

match contract.release_authorization {
    ReleaseAuthorization::ClientOnly => {
        if !is_client { env.panic_with_error(EscrowError::UnauthorizedRole); }
    }
    ReleaseAuthorization::ArbiterOnly => {
        if !is_arbiter { env.panic_with_error(EscrowError::UnauthorizedRole); }
    }
    ReleaseAuthorization::ClientAndArbiter => {
        if !is_client && !is_arbiter { env.panic_with_error(EscrowError::UnauthorizedRole); }
    }
    ReleaseAuthorization::MultiSig => {
        if !is_client && !is_freelancer { env.panic_with_error(EscrowError::UnauthorizedRole); }
    }
}
```

| Mode | Client | Freelancer | **Arbiter** |
|------|--------|------------|-------------|
| `ClientOnly` | ✅ | ❌ | ❌ |
| `ArbiterOnly` | ❌ | ❌ | ✅ |
| `ClientAndArbiter` | ✅ | ❌ | ✅ |
| `MultiSig` | ✅ | ✅ | ❌ |

**Required state for release:** `ContractStatus::Funded` (only — not `PartiallyFunded`).

**Approval sufficiency check (run inside `release_milestone` before funds move):**

Source: [`approvals.rs` L196–L205](../contracts/escrow/src/approvals.rs)

```rust
let sufficient = match contract.release_authorization {
    ReleaseAuthorization::ClientOnly       => approvals.client_approved,
    ReleaseAuthorization::ArbiterOnly      => approvals.arbiter_approved,
    ReleaseAuthorization::ClientAndArbiter => approvals.client_approved || approvals.arbiter_approved,
    ReleaseAuthorization::MultiSig         => approvals.client_approved && approvals.freelancer_approved,
};
```

---

## 5. Dispute Entrypoints

### 5.1 `raise_dispute`

Source: [`lib.rs` L2184–L2229](../contracts/escrow/src/lib.rs)

**Who may call:** Client **or** Freelancer — arbiter is **explicitly excluded**.

```rust
// Verify caller is client or freelancer
if caller != contract.client && caller != contract.freelancer {
    env.panic_with_error(Error::UnauthorizedRole);
}

// Require arbiter assignment
if contract.arbiter.is_none() {
    env.panic_with_error(Error::ArbiterRequired);
}
```

| Caller | Allowed? |
|--------|----------|
| Client | ✅ |
| Freelancer | ✅ |
| **Arbiter** | ❌ (`UnauthorizedRole`) |
| Admin | ❌ (`UnauthorizedRole`) |
| Other | ❌ (`UnauthorizedRole`) |

**Required contract state:** `Funded` or `PartiallyFunded`.  
**Pre-condition:** `Contract.arbiter` must be `Some(_)` — contracts without an
assigned arbiter cannot be put into dispute (`ArbiterRequired`).

**Transition:** `Funded | PartiallyFunded` → `Disputed`.

**Effect:** Blocks all further `release_milestone` calls until the arbiter
resolves the dispute.

---

### 5.2 `resolve_dispute`

Source: [`lib.rs` L2263–L2322](../contracts/escrow/src/lib.rs)

**Who may call:** Only the **assigned arbiter**.

```rust
arbiter.require_auth();

// Verify contract is in Disputed state
if contract.status != ContractStatus::Disputed {
    env.panic_with_error(Error::InvalidStatusTransition);
}

// Verify caller is the assigned arbiter
match &contract.arbiter {
    Some(contract_arbiter) if *contract_arbiter == arbiter => {}
    _ => env.panic_with_error(Error::UnauthorizedRole),
}
```

| Caller | Allowed? |
|--------|----------|
| **Arbiter** | ✅ (must match `Contract.arbiter`) |
| Client | ❌ (`UnauthorizedRole`) |
| Freelancer | ❌ (`UnauthorizedRole`) |
| Admin | ❌ (`UnauthorizedRole`) |

**Required contract state:** `Disputed` only.

**Resolution options (`DisputeResolution`):**

| Variant | Client receives | Freelancer receives |
|---------|-----------------|---------------------|
| `FullRefund` | 100% of available balance | 0 |
| `PartialRefund` | ~70% (remainder after 30% to freelancer) | 30% of available |
| `FullPayout` | 0 | 100% of available balance |
| `Split(client_amount, freelancer_amount)` | `client_amount` | `freelancer_amount` (must sum to available) |

`available = funded_amount − released_amount − refunded_amount`

**Transition:** `Disputed` → `Completed` (if any payout went to freelancer, or
partial mix) or `Refunded` (if `refunded_amount == funded_amount` after resolution).

**Side-effect:** If the contract transitions to `Completed`, a pending reputation
credit is granted to the freelancer so the client can later call `issue_reputation`.

---

## 6. Finalization

**Entrypoint:** `finalize_contract` → `finalize::finalize_contract_impl`

Source: [`finalize.rs` L67–L74](../contracts/escrow/src/finalize.rs)

```rust
fn require_finalizer_role(env: &Env, contract: &Contract, finalizer: &Address) {
    let is_client     = *finalizer == contract.client;
    let is_freelancer = *finalizer == contract.freelancer;
    let is_arbiter    = contract.arbiter.clone().is_some_and(|a| a == *finalizer);
    if !is_client && !is_freelancer && !is_arbiter {
        env.panic_with_error(Error::UnauthorizedRole);
    }
}
```

| Caller | Allowed? |
|--------|----------|
| Client | ✅ |
| Freelancer | ✅ |
| **Arbiter** | ✅ |
| Admin | ❌ (`UnauthorizedRole`) |

**Required contract state:** `Completed` or `Disputed`.

**Effect:** Writes an immutable `FinalizationRecord` to storage. After this,
all further contract-specific mutations fail with `AlreadyFinalized`.

---

## 7. Entrypoints Where the Arbiter Has No Role

| Entrypoint | Who may call | Arbiter? |
|------------|-------------|----------|
| `initialize` | Admin | ❌ |
| `bind_settlement_token` | Admin | ❌ |
| `deposit_funds` | Client only | ❌ |
| `refund_unreleased_milestones` | Client only | ❌ |
| `cancel_contract` | Client only | ❌ |
| `issue_reputation` | Client only | ❌ |
| `propose_client_migration` | Current client | ❌ |
| `accept_client_migration` | New (proposed) client | ❌ |
| `pause` / `unpause` / `activate_emergency_pause` / `resolve_emergency` | Admin | ❌ |
| `withdraw_protocol_fees` | Admin | ❌ |

---

## 8. Error Codes Related to Arbiter Authorization

| Error | Code (`types::Error`) | Code (`EscrowError`) | When raised |
|-------|-----------------------|----------------------|-------------|
| `UnauthorizedRole` | 11 | 15 | Caller is not permitted for the operation in the current mode |
| `ArbiterRequired` | 42 | 25 | `raise_dispute` called but `Contract.arbiter` is `None` |
| `MissingArbiter` | 12 (types) | 35 | `create_contract` called with `ArbiterOnly` or `ClientAndArbiter` mode but no arbiter address |
| `InvalidArbiter` | 13 (types) | 36 | Arbiter address equals client or freelancer |
| `InvalidStatusTransition` | 41 | 24 | `resolve_dispute` called but contract is not in `Disputed` state |
| `InsufficientApprovals` | 20 | — | `release_milestone` called but required approvals are absent or expired |
| `AlreadyApproved` | 18 | — | Arbiter (or other party) has already approved the same milestone |

---

## 9. Approval TTL — Arbiter Considerations

Approvals recorded by `approve_milestone_release` are stored in Soroban **temporary
storage** and expire automatically.

| Constant | Value | Duration |
|----------|-------|----------|
| `PENDING_APPROVAL_TTL_LEDGERS` | 120,960 ledgers | ~7 days @ 5 s/ledger |
| `PENDING_APPROVAL_BUMP_THRESHOLD` | 17,280 ledgers | ~1 day |

If the arbiter's approval expires before `release_milestone` is called, the
approval is treated as absent (`InsufficientApprovals`). All parties — including
the arbiter — must re-approve.

---

## 10. Worked Example — ArbiterOnly Release Mode

This example walks through a complete lifecycle where the arbiter controls milestone
releases, and then a dispute is raised and resolved.

### Setup

```
client    = GAAA…
freelancer = GBBB…
arbiter   = GCCC…
milestones = [1_000_000 stroops, 2_000_000 stroops]
release_authorization = ArbiterOnly
```

### Step 1 — Create contract

```
create_contract(client=GAAA, freelancer=GBBB, arbiter=GCCC,
                milestones=[1_000_000, 2_000_000],
                release_authorization=ArbiterOnly)
```

- **Auth required:** `client.require_auth()` ✅
- **Check:** `ArbiterOnly` mode requires arbiter → `GCCC` is present ✅
- **Check:** arbiter ≠ client, arbiter ≠ freelancer ✅
- **Result:** `contract_id = 1`, status = `Created`

### Step 2 — Client deposits full amount

```
deposit_funds(contract_id=1, caller=GAAA, amount=3_000_000)
```

- **Auth:** none required beyond SAC transfer
- **Result:** status = `Funded`, `funded_amount = 3_000_000`

### Step 3 — Arbiter approves milestone 0

```
approve_milestone_release(contract_id=1, caller=GCCC, milestone_index=0)
```

- **Auth:** `caller.require_auth()` ✅
- **Mode check (`ArbiterOnly`):** `is_arbiter = true` ✅
- **State check:** `Funded` ✅
- **Result:** `arbiter_approved = true` stored in temporary storage (TTL ~7 days)

Attempting this as the **client (GAAA)**:
```
approve_milestone_release(contract_id=1, caller=GAAA, milestone_index=0)
→ Error: UnauthorizedRole
```

### Step 4 — Arbiter releases milestone 0

```
release_milestone(contract_id=1, caller=GCCC, milestone_index=0)
```

- **Auth:** `caller.require_auth()` ✅
- **Mode check (`ArbiterOnly`):** `is_arbiter = true` ✅
- **State check:** `Funded` ✅
- **Approval check:** `arbiter_approved = true` ✅
- **Result:** 1,000,000 stroops (minus protocol fee) transferred to freelancer;
  milestone 0 marked `released = true`; `released_amount += 1_000_000`

### Step 5 — Freelancer opens a dispute before milestone 1 is released

```
raise_dispute(contract_id=1, caller=GBBB)
```

- **Auth:** `caller.require_auth()` ✅
- **Role check:** `GBBB == contract.freelancer` ✅
- **Arbiter check:** `contract.arbiter = Some(GCCC)` ✅
- **State check:** `Funded` ✅
- **Result:** status = `Disputed`

Attempting this as the **arbiter (GCCC)**:
```
raise_dispute(contract_id=1, caller=GCCC)
→ Error: UnauthorizedRole  (arbiter is not client or freelancer)
```

### Step 6 — Arbiter resolves the dispute

Available balance = `funded_amount − released_amount − refunded_amount`
                  = `3_000_000 − 1_000_000 − 0 = 2_000_000`

```
resolve_dispute(
    contract_id=1,
    arbiter=GCCC,
    resolution=Split { client_amount=800_000, freelancer_amount=1_200_000 }
)
```

- **Auth:** `arbiter.require_auth()` ✅
- **State check:** `Disputed` ✅
- **Arbiter identity:** `GCCC == contract.arbiter.unwrap()` ✅
- **Split validation:** `800_000 + 1_200_000 = 2_000_000 == available` ✅
- **Result:**
  - 800,000 stroops transferred to client → `refunded_amount += 800_000`
  - 1,200,000 stroops transferred to freelancer → `released_amount += 1_200_000`
  - `released_amount (2_200_000) != funded_amount (3_000_000)` → status = `Completed`
  - Pending reputation credit granted to `GBBB`

### Step 7 — Arbiter finalizes the contract

```
finalize_contract(contract_id=1, finalizer=GCCC)
```

- **Auth:** `finalizer.require_auth()` ✅
- **Role check:** `GCCC == contract.arbiter.unwrap()` ✅
- **State check:** `Completed` ✅
- **Result:** `FinalizationRecord` written; contract is immutably closed

---

## 11. Rejection Summary

The following table consolidates every guard that rejects an arbiter (or rejects
*because* an arbiter is absent):

| Entrypoint | Condition | Error |
|------------|-----------|-------|
| `create_contract` | Mode is `ArbiterOnly` or `ClientAndArbiter` and `arbiter = None` | `MissingArbiter` |
| `create_contract` | Arbiter equals client or freelancer | `InvalidArbiter` |
| `approve_milestone_release` | Contract not `Funded`/`PartiallyFunded` | `InvalidState` |
| `approve_milestone_release` | Mode is `ClientOnly` or `MultiSig`, caller is arbiter | `UnauthorizedRole` |
| `approve_milestone_release` | Arbiter already approved the same milestone | `AlreadyApproved` |
| `release_milestone` | Contract not `Funded` | `InvalidState` |
| `release_milestone` | Mode is `ClientOnly` or `MultiSig`, caller is arbiter | `UnauthorizedRole` |
| `release_milestone` | Approvals missing or expired | `InsufficientApprovals` |
| `raise_dispute` | Caller is arbiter (not client/freelancer) | `UnauthorizedRole` |
| `raise_dispute` | `Contract.arbiter = None` | `ArbiterRequired` |
| `raise_dispute` | Contract not `Funded`/`PartiallyFunded` | `InvalidState` |
| `resolve_dispute` | Contract not `Disputed` | `InvalidStatusTransition` |
| `resolve_dispute` | Caller ≠ assigned arbiter | `UnauthorizedRole` |
| `resolve_dispute` | Split amounts don't conserve available balance | `InvalidDisputeSplit` |
| `finalize_contract` | Caller is not client, freelancer, or arbiter | `UnauthorizedRole` |
| `finalize_contract` | Contract not `Completed`/`Disputed` | `InvalidStatusTransition` |

---

## 12. Source Cross-Reference

| Entrypoint | Source file | Key lines |
|------------|-------------|-----------|
| `create_contract` | `contracts/escrow/src/create_contract.rs` | L41–L174 |
| `approve_milestone_release` → `approve_milestone` | `contracts/escrow/src/approvals.rs` | L46–L158 |
| `check_approvals` | `contracts/escrow/src/approvals.rs` | L180–L212 |
| `release_milestone` | `contracts/escrow/src/lib.rs` | L690–L900 |
| `raise_dispute` | `contracts/escrow/src/lib.rs` | L2184–L2229 |
| `resolve_dispute` | `contracts/escrow/src/lib.rs` | L2263–L2322 |
| `finalize_contract` → `finalize_contract_impl` | `contracts/escrow/src/finalize.rs` | L140–L168 |
| `ReleaseAuthorization` enum | `contracts/escrow/src/types.rs` | L246–L256 |
| `ContractStatus` enum | `contracts/escrow/src/types.rs` | L200–L210 |
| `DisputeResolution` enum | `contracts/escrow/src/types.rs` | L337–L354 |
| `Contract` struct | `contracts/escrow/src/types.rs` | L213–L226 |
| `Error` enum | `contracts/escrow/src/types.rs` | L96–L196 |
| `EscrowError` enum | `contracts/escrow/src/lib.rs` | L102–L173 |
