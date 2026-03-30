# Escrow Contract — Event Reference

The TalentTrust Escrow contract emits structured events for every state-changing operation.  A successful call emits its events *after* all state is written; a failing call emits *no* events.

Events are published via `env.events().publish(topics, data)` where:

- **topics** — a 2-tuple `(namespace: Symbol, operation: Symbol)` that uniquely identifies the event type.
- **data** — the event payload, either a scalar value or a serialised tuple.

All symbols are `symbol_short!` values (≤ 9 UTF-8 bytes) which fit in 56-bit XDR.

---

## Event catalogue

### Pause-control events

| Topics | Data | Emitted by |
|--------|------|------------|
| `("pause", "init")` | `admin: Address` | `initialize` |
| `("pause", "pause")` | `admin: Address` | `pause` |
| `("pause", "unpause")` | `admin: Address` | `unpause` |
| `("pause", "emerg")` | `admin: Address` | `activate_emergency_pause` |
| `("pause", "resolv")` | `admin: Address` | `resolve_emergency` |

**`("pause", "init")`** — emitted once when pause controls are bootstrapped.  The data is the address of the pause-control administrator.

**`("pause", "pause")` / `("pause", "unpause")`** — pair that records normal pause state transitions.

**`("pause", "emerg")`** — signals that emergency mode activated; simultaneously sets both `Paused` and `EmergencyPaused` storage flags.  Use `resolve_emergency` (not `unpause`) to clear this state.

**`("pause", "resolv")`** — signals that emergency mode was cleared; both flags are reset.

---

### Governance events

| Topics | Data | Emitted by |
|--------|------|------------|
| `("gov", "init")` | `admin: Address` | `initialize_protocol_governance` |
| `("gov", "params")` | `(min_milestone_amount: i128, max_milestones: u32, min_reputation_rating: i128, max_reputation_rating: i128)` | `update_protocol_parameters` |
| `("gov", "propose")` | `new_admin: Address` | `propose_governance_admin` |
| `("gov", "accept")` | `new_admin: Address` | `accept_governance_admin` |

**`("gov", "init")`** — one-time emission when governance is bootstrapped; data is the initial admin.

**`("gov", "params")`** — emitted on every parameter update.  The 4-tuple data encodes the *new* active values in this order: `(min_milestone_amount, max_milestones, min_reputation_rating, max_reputation_rating)`.

**`("gov", "propose")` / `("gov", "accept")`** — two-step admin handover.  A pending transfer in the `proposed` state cannot interact with contract operations until `accept` is called by the proposed admin.

---

### Escrow core events

| Topics | Data | Emitted by |
|--------|------|------------|
| `("escrow", "create")` | `(id: u32, client: Address, freelancer: Address, total_amount: i128)` | `create_contract` |
| `("escrow", "deposit")` | `(id: u32, amount: i128, funded_amount: i128)` | `deposit_funds` |
| `("escrow", "release")` | `(id: u32, milestone_id: u32, amount: i128)` | `release_milestone` |
| `("escrow", "complete")` | `id: u32` | `release_milestone` (last milestone only) |
| `("escrow", "rep")` | `(id: u32, freelancer: Address, rating: i128)` | `issue_reputation` |

**`("escrow", "create")`** — emitted when a new escrow agreement is stored.  `id` is a monotonically incrementing `u32` starting at 1.  `total_amount` is the sum of all milestone amounts.

**`("escrow", "deposit")`** — emitted for every successful deposit.  `amount` is the deposit amount for this invocation; `funded_amount` is the *cumulative* total deposited against the contract.

**`("escrow", "release")`** — emitted when a single milestone payment is released.  `amount` is the per-milestone amount defined at contract creation.

**`("escrow", "complete")`** — emitted *in the same invocation* as `("escrow", "release")` when the *last* milestone is released; always follows the `release` event in the event list.  The data is just the `u32` contract ID (not a tuple).

**`("escrow", "rep")`** — emitted when a reputation credential is issued for the freelancer of a completed contract.  Can only be emitted once per contract ID.

---

## Ordering guarantees

An invocation that both releases the final milestone and transitions the contract to `Completed` emits exactly **two** events in this order:

```
[0]: ("escrow", "release")  → (id, last_milestone_id, amount)
[1]: ("escrow", "complete") → id
```

No other operation emits more than one event per invocation.

---

## Decoding payloads

Events are encoded as Soroban host values.  In tests, use `TryFromVal` to decode back to concrete types:

```rust
use soroban_sdk::TryFromVal;

let (_, _, data) = env.events().all().get(0).unwrap();

// Decode address data
let admin = Address::try_from_val(&env, &data).unwrap();

// Decode tuple data
let (id, client, freelancer, total): (u32, Address, Address, i128) =
    <(u32, Address, Address, i128)>::try_from_val(&env, &data).unwrap();
```

---

## Absence guarantees

A failed operation (returning an `EscrowError`) emits **no events**.  Integrators can therefore rely on the *absence* of an event as a signal that no state change occurred.

| Error                   | Emits event? |
|-------------------------|--------------|
| `ContractNotFound`      | No           |
| `MilestoneNotFound`     | No           |
| `InvalidAmount`         | No           |
| `InvalidRating`         | No           |
| `EmptyMilestones`       | No           |
| `InvalidParticipants`   | No           |
| `FundingExceedsRequired`| No           |
| `InvalidState`          | No           |
| `InsufficientEscrowBalance` | No       |
| `MilestoneAlreadyReleased` | No        |
| `ReputationAlreadyIssued` | No         |

---

## Error codes

| Code | Variant | `Error(Contract, #N)` |
|------|---------|-----------------------|
| 1 | `ContractNotFound` | `#1` |
| 2 | `MilestoneNotFound` | `#2` |
| 3 | `InvalidAmount` | `#3` |
| 4 | `InvalidRating` | `#4` |
| 5 | `EmptyMilestones` | `#5` |
| 6 | `InvalidParticipants` | `#6` |
| 7 | `FundingExceedsRequired` | `#7` |
| 8 | `InvalidState` | `#8` |
| 9 | `InsufficientEscrowBalance` | `#9` |
| 10 | `MilestoneAlreadyReleased` | `#10` |
| 11 | `ReputationAlreadyIssued` | `#11` |
