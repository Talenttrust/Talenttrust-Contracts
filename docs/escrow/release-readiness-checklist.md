# Release Readiness Checklist — Escrow Contract

**Issue:** #50  
**Contract:** `contracts/escrow`  
**Framework:** Soroban (Stellar), `soroban-sdk` v22.0

---

## Overview

The **Release Readiness Checklist** is an on-chain mechanism that automatically
tracks whether a TalentTrust escrow agreement has passed all deployment,
verification, and post-deploy monitoring gates before funds can be released.

Checklist state is stored as a `ReleaseChecklist` struct (one per contract ID)
and is **automatically updated by contract operations only** — no external
caller can set items directly, preventing unauthorised state manipulation.

---

## Phases and Items

### Phase 1 — Deployment

| Item | Field | Set By |
|---|---|---|
| Contract has been successfully created and persisted | `contract_created` | `create_contract` |
| Client has deposited a positive amount into escrow | `funds_deposited` | `deposit_funds` |

### Phase 2 — Verification

| Item | Field | Set By |
|---|---|---|
| Both client and freelancer addresses have been recorded | `parties_authenticated` | `create_contract` |
| At least one milestone amount has been defined | `milestones_defined` | `create_contract` |

### Phase 3 — Post-Deploy Monitoring

| Item | Field | Set By |
|---|---|---|
| Every milestone in the agreement has been released | `all_milestones_released` | `release_milestone` (last call) |
| A reputation credential has been issued for the freelancer | `reputation_issued` | `issue_reputation` |

---

## Enforcement

`release_milestone` is **hard-enforced**: it panics with `EscrowError::ChecklistIncomplete`
if any Phase 1 or Phase 2 item is `false`.  This means milestone payments cannot
be released until:

1. `create_contract` has been called (sets `contract_created`, `parties_authenticated`, `milestones_defined`).
2. `deposit_funds` has been called with a positive amount (sets `funds_deposited`).

Phase 3 items (`all_milestones_released`, `reputation_issued`) are informational
post-deploy monitors — they do not block any operations.

---

## Function Reference

### `create_contract`

```rust
pub fn create_contract(
    env: Env,
    client: Address,
    freelancer: Address,
    milestone_amounts: Vec<i128>,
) -> u32
```

Creates an escrow agreement and returns a unique `contract_id`.  
Automatically sets `contract_created`, `parties_authenticated`, `milestones_defined`.

**Constraints:**
- `milestone_amounts` must be non-empty and have ≤ `MAX_MILESTONES` (20) entries.
- `client` and `freelancer` must be distinct identities.
- Every milestone amount must be strictly positive (`> 0`).
- Panics with `TooManyMilestones`, `DuplicateIdentities`, or `InvalidMilestoneAmount` on invalid input.

---

### `deposit_funds`

```rust
pub fn deposit_funds(env: Env, contract_id: u32, amount: i128) -> bool
```

Records a client deposit and advances status to `Funded`.  
Automatically sets `funds_deposited`.

**Constraints:**
- `amount` must be > 0; panics with `InvalidDepositAmount` otherwise.
- Panics with `ContractNotFound` for unknown `contract_id`.

---

### `release_milestone`

```rust
pub fn release_milestone(env: Env, contract_id: u32, milestone_id: u32) -> bool
```

Releases the payment for one milestone.  
Sets `all_milestones_released` when the last milestone is released.

**Enforcement guard:** panics with `ChecklistIncomplete` if `is_release_ready` returns `false`.

**Additional panics:**
- `ContractNotFound` — unknown `contract_id`.
- `InvalidMilestoneId` — `milestone_id` out of range.
- `MilestoneAlreadyReleased` — milestone was already released.

---

### `issue_reputation`

```rust
pub fn issue_reputation(env: Env, contract_id: u32, _freelancer: Address, _rating: i128) -> bool
```

Records that a reputation credential has been issued.  
Sets `reputation_issued`.

**Panics:** `ContractNotFound` for unknown `contract_id`.

---

### `get_release_checklist`

```rust
pub fn get_release_checklist(env: Env, contract_id: u32) -> ReleaseChecklist
```

Returns the full `ReleaseChecklist` struct for a contract.  
Useful for off-chain dashboards and pre-release verification scripts.

---

### `is_release_ready`

```rust
pub fn is_release_ready(env: Env, contract_id: u32) -> bool
```

Returns `true` when all Phase 1 + Phase 2 items are satisfied:
- `contract_created && funds_deposited && parties_authenticated && milestones_defined`

---

### `is_post_deploy_complete`

```rust
pub fn is_post_deploy_complete(env: Env, contract_id: u32) -> bool
```

Returns `true` when all six checklist items are satisfied — indicating the full
deployment lifecycle, payment settlement, and reputation issuance are complete.

---

## Error Codes

| Code | Value | Meaning |
|---|---|---|
| `ContractNotFound` | 1 | `contract_id` does not exist in storage |
| `InvalidMilestoneId` | 2 | `milestone_id` is out of range |
| `MilestoneAlreadyReleased` | 3 | Milestone was already released |
| `ChecklistIncomplete` | 4 | Release gate not satisfied |
| `InvalidDepositAmount` | 5 | Deposit amount ≤ 0 |
| `TooManyMilestones` | 6 | Milestone count exceeds `MAX_MILESTONES` (20) |
| `DuplicateIdentities` | 7 | `client` and `freelancer` are the same address |
| `InvalidMilestoneAmount` | 8 | One or more milestone amounts are ≤ 0 |

---

## Security Model

- **No manual overrides:** `ReleaseChecklist` fields can only be flipped to `true` by
  the specific contract function that naturally satisfies each gate.  There is no
  `set_checklist_item` function, preventing privilege escalation.
- **Monotonic IDs:** Contract IDs are generated by a persistent counter
  (`DataKey::NextId`) that increments atomically — no ID reuse.
- **Input validation at boundaries:** Deposit amounts and milestone counts are
  validated at the contract boundary before any state is written.
- **Identity collision prevention:** Contract creation rejects `client == freelancer`
  to avoid role-collision ambiguity and self-dealing pathways.
- **Malformed payload rejection:** Contract creation rejects non-positive
  milestone values to prevent invalid financial state from being persisted.
- **Duplicate-release protection:** `release_milestone` panics with
  `MilestoneAlreadyReleased` on re-entrancy or double-spend attempts.

---

## Example Lifecycle

```text
create_contract(client, freelancer, [100, 200, 300])
  → contract_id = 1
  → checklist: contract_created=T, parties_authenticated=T, milestones_defined=T
               funds_deposited=F, all_milestones_released=F, reputation_issued=F

deposit_funds(1, 600)
  → checklist: funds_deposited=T  (is_release_ready → true)

release_milestone(1, 0)   # 100 released
release_milestone(1, 1)   # 200 released
release_milestone(1, 2)   # 300 released → all_milestones_released=T

issue_reputation(1, freelancer, 5)
  → checklist: reputation_issued=T  (is_post_deploy_complete → true)
```

---

## Testing

Checklist and input-sanitization behaviour is covered in `contracts/escrow/src/test.rs`
and dedicated modules under `contracts/escrow/src/test/`:

| Test | Covers |
|---|---|
| `test_create_contract_panics_when_client_equals_freelancer` | Duplicate identity rejection |
| `test_create_contract_panics_when_single_milestone_is_zero` | Non-positive milestone rejection (zero) |
| `test_create_contract_panics_when_single_milestone_is_negative` | Non-positive milestone rejection (negative) |
| `test_create_contract_panics_when_any_milestone_is_non_positive` | Mixed malformed payload rejection |
| `test_checklist_initialized_on_create` | 3 items auto-set on creation |
| `test_deposit_funds_sets_checklist_flag` | `funds_deposited` updated |
| `test_is_release_ready_false_before_deposit` | Gate enforcement (false path) |
| `test_is_release_ready_true_after_deposit` | Gate enforcement (true path) |
| `test_release_blocked_before_deposit` | `#[should_panic]` — `ChecklistIncomplete` |
| `test_release_milestone_succeeds_when_ready` | Happy path release |
| `test_all_milestones_released_flag_set_after_last_release` | Post-deploy flag |
| `test_issue_reputation_updates_checklist` | `reputation_issued` updated |
| `test_is_post_deploy_complete_full_lifecycle` | End-to-end all 6 items |
| `test_get_release_checklist_reflects_state_progression` | Getter accuracy |
| `test_independent_contracts_do_not_share_checklist_state` | Storage isolation |
