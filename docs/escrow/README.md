# Escrow Contract — Developer Reference

Soroban smart contract implementing milestone-based escrow for the TalentTrust
freelancer protocol on the Stellar network.

---

## Table of Contents

1. [Overview](#overview)
2. [Data Types](#data-types)
3. [Public Methods](#public-methods)
4. [Authorization Schemes](#authorization-schemes)
5. [Contract Lifecycle](#contract-lifecycle)
6. [Failure Scenarios](#failure-scenarios)
7. [Security Notes](#security-notes)

---

## Overview

The escrow contract holds funds on behalf of a **client** and releases them to
a **freelancer** as individual milestones are approved. An optional **arbiter**
can be designated for dispute resolution or multi-signature flows.

---

## Data Types

### `ContractStatus`

| Variant     | Value | Description                                              |
|-------------|-------|----------------------------------------------------------|
| `Created`   | 0     | Contract created; awaiting client deposit.               |
| `Funded`    | 1     | Client has deposited the full escrow amount.             |
| `Completed` | 2     | All milestones released; contract is closed.             |
| `Disputed`  | 3     | Dispute raised; funds frozen pending resolution.         |

### `ReleaseAuthorization`

| Variant            | Value | Who may approve / release                        |
|--------------------|-------|--------------------------------------------------|
| `ClientOnly`       | 0     | Client only.                                     |
| `ClientAndArbiter` | 1     | Client **or** arbiter (either suffices).         |
| `ArbiterOnly`      | 2     | Arbiter only.                                    |
| `MultiSig`         | 3     | Both client and arbiter (partial implementation).|

### `Milestone`

| Field                | Type              | Description                                      |
|----------------------|-------------------|--------------------------------------------------|
| `amount`             | `i128`            | Payment amount in stroops.                       |
| `released`           | `bool`            | Whether payment has been released.               |
| `approved_by`        | `Option<Address>` | Last approver address.                           |
| `approval_timestamp` | `Option<u64>`     | Ledger timestamp of approval.                    |

### `EscrowContract`

| Field         | Type                   | Description                                      |
|---------------|------------------------|--------------------------------------------------|
| `client`      | `Address`              | Funds the escrow.                                |
| `freelancer`  | `Address`              | Receives milestone payments.                     |
| `arbiter`     | `Option<Address>`      | Optional dispute / multi-sig party.              |
| `milestones`  | `Vec<Milestone>`       | Ordered milestone list.                          |
| `status`      | `ContractStatus`       | Current lifecycle state.                         |
| `release_auth`| `ReleaseAuthorization` | Authorization scheme.                            |
| `created_at`  | `u64`                  | Ledger timestamp at creation.                    |

---

## Public Methods

### `create_contract`

```
create_contract(
    env: Env,
    client: Address,
    freelancer: Address,
    arbiter: Option<Address>,
    milestone_amounts: Vec<i128>,
    release_auth: ReleaseAuthorization,
) -> u32
```

Creates a new escrow record and returns a contract ID (ledger sequence number).

**Panics:**
- `"At least one milestone required"` — `milestone_amounts` is empty.
- `"Client and freelancer cannot be the same address"` — identical addresses.
- `"Milestone amounts must be positive"` — any amount ≤ 0.

---

### `deposit_funds`

```
deposit_funds(env: Env, _contract_id: u32, caller: Address, amount: i128) -> bool
```

Deposits the full escrow amount. Transitions status `Created → Funded`.

**Requires auth:** `caller` (must be the client).

**Panics:**
- `"Contract not found"` — no contract in storage.
- `"Only client can deposit funds"` — caller is not the client.
- `"Contract must be in Created status to deposit funds"` — wrong status.
- `"Deposit amount must equal total milestone amounts"` — amount mismatch.

---

### `approve_milestone_release`

```
approve_milestone_release(
    env: Env,
    _contract_id: u32,
    caller: Address,
    milestone_id: u32,
) -> bool
```

Records an approval for a milestone. Does **not** release funds.

**Requires auth:** `caller`.

**Panics:**
- `"Contract not found"` — no contract in storage.
- `"Contract must be in Funded status to approve milestones"` — wrong status.
- `"Invalid milestone ID"` — `milestone_id` out of range.
- `"Milestone already released"` — milestone already paid out.
- `"Caller not authorized to approve milestone release"` — not permitted under `release_auth`.
- `"Milestone already approved by this address"` — duplicate approval.

---

### `release_milestone`

```
release_milestone(
    env: Env,
    _contract_id: u32,
    caller: Address,
    milestone_id: u32,
) -> bool
```

Releases a milestone payment to the freelancer. Transitions contract to
`Completed` when all milestones are released.

**Requires auth:** `caller`.

**Panics:**
- `"Contract not found"` — no contract in storage.
- `"Contract must be in Funded status to release milestones"` — wrong status.
- `"Invalid milestone ID"` — `milestone_id` out of range.
- `"Milestone already released"` — already paid out.
- `"Insufficient approvals for milestone release"` — required approvals absent.

---

### `issue_reputation`

```
issue_reputation(_env: Env, _freelancer: Address, _rating: i128) -> bool
```

Stub for on-chain reputation credential issuance. Always returns `true`.

---

### `hello`

```
hello(_env: Env, to: Symbol) -> Symbol
```

Echo function for smoke-testing. Returns the input symbol unchanged.

---

## Authorization Schemes

| Scheme             | `approve_milestone_release` caller | `release_milestone` condition     |
|--------------------|------------------------------------|-----------------------------------|
| `ClientOnly`       | Client                             | Client has approved               |
| `ArbiterOnly`      | Arbiter                            | Arbiter has approved              |
| `ClientAndArbiter` | Client or Arbiter                  | Client or Arbiter has approved    |
| `MultiSig`         | Client or Arbiter                  | Client has approved (stub)        |

---

## Contract Lifecycle

```
create_contract
      │
      ▼
  [Created]
      │  deposit_funds (client, full amount)
      ▼
  [Funded]
      │  approve_milestone_release  ←──────────────┐
      │  release_milestone                          │
      │  (repeat for each milestone)  ──────────────┘
      ▼
 [Completed]  ← all milestones released
```

---

## Failure Scenarios

| Scenario                              | Method                       | Panic message                                              |
|---------------------------------------|------------------------------|------------------------------------------------------------|
| Empty milestone list                  | `create_contract`            | `"At least one milestone required"`                        |
| Client == freelancer                  | `create_contract`            | `"Client and freelancer cannot be the same address"`       |
| Non-positive milestone amount         | `create_contract`            | `"Milestone amounts must be positive"`                     |
| Non-client attempts deposit           | `deposit_funds`              | `"Only client can deposit funds"`                          |
| Deposit on already-funded contract    | `deposit_funds`              | `"Contract must be in Created status to deposit funds"`    |
| Wrong deposit amount                  | `deposit_funds`              | `"Deposit amount must equal total milestone amounts"`      |
| Unauthorised approver                 | `approve_milestone_release`  | `"Caller not authorized to approve milestone release"`     |
| Duplicate approval                    | `approve_milestone_release`  | `"Milestone already approved by this address"`             |
| Out-of-range milestone ID             | `approve_milestone_release`  | `"Invalid milestone ID"`                                   |
| Release without approval              | `release_milestone`          | `"Insufficient approvals for milestone release"`           |
| Double release                        | `release_milestone`          | `"Milestone already released"`                             |

---

## Security Notes

- **Authentication:** Every state-mutating method calls `caller.require_auth()`,
  ensuring the Stellar network validates the caller's signature.
- **Single-record storage:** The current implementation stores one escrow record
  under the key `"contract"`. A production deployment must key by `contract_id`
  to support concurrent escrows.
- **No token transfer:** Fund custody and transfer to the freelancer must be
  implemented via the Stellar asset contract; this contract only tracks state.
- **MultiSig stub:** The `MultiSig` variant currently behaves like `ClientOnly`
  at release time. Full multi-signature enforcement is a planned enhancement.
