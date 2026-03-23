# Partial Refund — Escrow Contract

> Reviewer guide for `request_refund` — added in `feature/contracts-10-partial-refund-logic`

## Purpose

Allows a client to recover the deposit for all **unreleased** milestones when a
project is cancelled or stalls. Milestones that have already been paid out to
the freelancer are not reversed.

## Entrypoint

```rust
pub fn request_refund(env: Env, contract_id: u32) -> i128
```

| Parameter | Type | Description |
|---|---|---|
| `contract_id` | `u32` | ID returned by `create_contract` |

**Returns:** the number of stroops refunded (1 XLM = 10 000 000 stroops).

## State Transitions

```
Created ──► Funded ──► Completed   (all milestones released)
                  └──► Cancelled   (request_refund called)
```

`request_refund` can only be called while the contract is in `Created` or
`Funded` status. Once cancelled the contract is terminal.

## Refund Amount Calculation

```
refund_amount = Σ milestone.amount  for each milestone where milestone.released == false
```

The amount is derived entirely from on-chain data stored at contract creation.
No caller-supplied amount is accepted or trusted.

## Access Control

| Condition | Behaviour |
|---|---|
| Caller ≠ client | Panics — `require_auth` fails |
| Status == `Completed` | Panics — `InvalidStatus` |
| Status == `Cancelled` | Panics — `InvalidStatus` (double-refund guard) |
| All milestones released, status still Funded | Panics — `NothingToRefund` |

## Event Emitted

```
topics: ("refund", contract_id: u32)
data:   refund_amount: i128
```

Off-chain indexers should listen for this event to update client dashboards and
accounting.

## Security Threat Model

| Threat | Mitigation |
|---|---|
| Freelancer calls refund | `require_auth` on `client`; rejected |
| Client refunds twice | Status set to `Cancelled` after first call; second call panics |
| Client refunds after completion | `Completed` status guard; panics |
| Client inflates refund amount | Amount computed from immutable milestone list; no input trusted |
| Reentrancy | Soroban's single-threaded execution model prevents reentrancy |

## Production Token Integration

The current implementation tracks balances internally. In a production
deployment, replace the internal balance decrement with a token transfer:

```rust
token::Client::new(&env, &token_address)
    .transfer(&env.current_contract_address(), &data.client, &refund_amount);
```

## Usage Example

```bash
# 1. Create a 3-milestone contract
stellar contract invoke --id $ESCROW_ID -- \
  create_contract \
    --client GCLIENT... \
    --freelancer GFREELANCER... \
    --milestone_amounts '[200000000, 400000000, 600000000]'
# → contract_id = 1

# 2. Fund it
stellar contract invoke --id $ESCROW_ID -- \
  deposit_funds --contract_id 1 --amount 1200000000

# 3. Release first milestone (work delivered)
stellar contract invoke --id $ESCROW_ID -- \
  release_milestone --contract_id 1 --milestone_id 0

# 4. Project stalls — client requests refund
stellar contract invoke --id $ESCROW_ID -- \
  request_refund --contract_id 1
# → returns 1000000000 (400 + 600 XLM)
```

## Test Coverage

All tests live in `contracts/escrow/src/test.rs`.

| Test | Scenario |
|---|---|
| `test_refund_full_unreleased` | No milestones released → full refund |
| `test_refund_partial_unreleased_one_released` | 1 of 3 released → refund remaining 2 |
| `test_refund_partial_unreleased_two_released` | 2 of 3 released → refund last 1 |
| `test_deposit_then_immediate_refund` | Fund then cancel immediately |
| `test_refund_single_milestone` | Single-milestone contract |
| `test_refund_emits_event` | Event emitted with correct topics |
| `test_refund_fails_when_completed` | Completed guard |
| `test_refund_fails_when_already_cancelled` | Double-refund guard |
| `test_refund_fails_unauthorized` | Auth guard |
| `test_refund_fails_zero_unreleased` | NothingToRefund guard |
