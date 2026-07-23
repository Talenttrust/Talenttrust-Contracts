# Settlement Ordering

This document describes the **payout order, rounding rules, and accounting guarantees**
for every settlement path in the escrow contract. Each section covers one entrypoint,
the sequence in which funds move, how rounding is applied, and how disputes or
holds alter the baseline.

---

## 1. Normal Settlement: Milestone Release (`release_milestone`)

The canonical settlement path. Funds flow **freelancer-first**: the net payout
is transferred out of the escrow before the protocol fee is accrued.

### Ordering

1. **Preflight validation** — pause/emergency gate, caller auth, `require_not_finalized`,
   contract status must be `Funded`, caller role checked against `ReleaseAuthorization`,
   milestone index bounds, milestone not already released/refunded, valid approvals
   present and not expired.
2. **Available-balance check** — `available = funded_amount - released_amount - refunded_amount`
   must be at least `milestone.amount`.
3. **Protocol-fee computation** — `fee = floor(amount * fee_bps / 10_000)`.
   Short-circuits to `0` when `fee_bps == 0`.
4. **Net-amount derivation** — `net = amount - fee`.
5. **Escrow-commingled-balance check** — `available - accumulated_fees` must be at least
   `gross_amount` (the full milestone amount, not the net). This ensures that
   already-accrued fees are not accidentally considered spendable.
6. **SAC transfer: escrow → freelancer** — `token.transfer(escrow, freelancer, net)`.
7. **Fee accrual** — `AccumulatedProtocolFees += fee`.
8. **Milestone-state update** — `milestone.released = true`.
9. **Contract accounting update** — `released_amount += net`.
10. **Accounting-invariant check** — `released_amount + refunded_amount + total_accumulated_fees <= funded_amount`.
11. **Approval cleanup** — milestone approvals removed from temporary storage.
12. **Completion check** — if all milestones are terminal (released or refunded),
    status becomes `Completed` and one pending reputation credit is granted to the
    freelancer.
13. **Event emission** — `mlstn_rls` (per release) and optionally `ctrct_cmp`
    (when contract completes).

### Rounding

Protocol fees use **integer floor division**:

```
fee = amount * fee_bps / 10_000
```

The fee always rounds **down**. The freelancer always receives `amount - fee`,
which is strictly ≥ `amount - ceil(amount * fee_bps / 10_000)`. There is never
a case where the protocol takes more than the basis-point rate.

### Held funds

Milestone releases are blocked while the contract is `Paused` or under
`EmergencyActive`. Approvals expire after `PENDING_APPROVAL_TTL_LEDGERS` (~7 days).
A hold does not cancel the milestone — once the hold is lifted and fresh
approvals are obtained, the release proceeds normally.

---

## 2. Full Contract Cancellation (`cancel_contract`)

Cancellation returns **all remaining funds** to the client in a single transfer.
Only allowed before any milestone has been released (`released_amount == 0`).

### Ordering

1. **Preflight validation** — pause/emergency gate, `require_not_finalized`,
   caller must be the stored client, status must be `Created` or `Funded`,
   `released_amount` must be `0`.
2. **Refund-amount calculation** — `refund = funded_amount - released_amount - refunded_amount`.
3. **SAC transfer: escrow → client** — `token.transfer(escrow, client, refund)`.
   If `refund == 0` (contract created but never funded), the transfer is skipped.
4. **Contract accounting update** — `refunded_amount += refund`.
5. **Status update** — status set to `Cancelled`.
6. **Event emission** — `cancelled`.

### Rounding

No rounding applies — the entire `refundable_balance` is returned atomically.

### Held funds

Cancellation is blocked while paused or in emergency. Since cancellation
transfers the full balance, it cannot be partial.

---

## 3. Per-Milestone Refund (`refund_unreleased_milestones`)

Selectively refunds one or more unreleased milestones to the client. Milestone
deadlines are enforced: if a milestone has a `deadline` timestamp, the current
ledger time must be strictly past the deadline before a refund is allowed.

### Ordering

1. **Preflight validation** — pause/emergency gate, request not empty, no
   duplicate indices, contract exists, `require_not_finalized`, status must be
   `Created`, `Funded`, or `Disputed`.
2. **Auth** — `client.require_auth()`.
3. **Per-milestone validation** — each index in bounds, milestone not already
   released, not already refunded, deadline check (if a deadline is set, the
   milestone must be overdue: `now > deadline`).
4. **Total-refund calculation** — `total_refund = sum(milestone.amount for each
   requested milestone)`.
5. **Available-balance check** — `funded_amount - released_amount - refunded_amount`
   must be ≥ `total_refund`.
6. **SAC transfer: escrow → client** — `token.transfer(escrow, client, total_refund)`.
7. **Milestone-state update** — each requested milestone marked `refunded = true`.
8. **Contract accounting update** — `refunded_amount += total_refund`.
9. **Status transition**:
   - All milestones terminal and all refunded → `Refunded`
   - All milestones terminal, mixed (some released, some refunded) → `Completed`
   - Otherwise → unchanged `Funded` / `PartiallyFunded` / `Disputed`
10. **Event emission** — `refunded`.

### Rounding

No rounding applies — each milestone amount is an integer `i128` stroop value,
and refunds transfer the exact milestone amounts requested.

### Held funds

Refunds are blocked while paused or in emergency. A refund does not affect
released milestones. If a milestone has no deadline (`None`), a refund is
allowed at any time (backward-compatible behavior).

---

## 4. Dispute Resolution (`resolve_dispute`)

Dispute resolution is an **accounting-only** operation: the arbiter's decision
determines who owns the available balance, and the contract records that
allocation by updating `released_amount` and `refunded_amount`. **No tokens
move** during dispute resolution — the escrow's SAC balance remains unchanged.
Token extraction happens implicitly: the freelancer's awarded share is
reflected in `released_amount` (per the accounting invariant), and the client's
awarded share in `refunded_amount`.

### Ordering

1. **Preflight validation** — pause/emergency gate, `require_not_finalized`,
   status must be `Disputed`, caller must be the assigned `arbiter`.
2. **Available-balance computation** — `available = funded_amount - released_amount - refunded_amount`.
3. **Payout calculation** via `resolution_payouts` (pure arithmetic):
   - `FullRefund` → `(available, 0)` — all to client. Final status: `Refunded`.
   - `FullPayout` → `(0, available)` — all to freelancer. Final status: `Completed`.
   - `PartialRefund` → `(available - floor(available * 30 / 100), floor(available * 30 / 100))` — 70/30 split. Final status: `Completed`.
   - `Split(client_amt, freelancer_amt)` → `(client_amt, freelancer_amt)` — custom. Final status: `Completed` (unless full refund).
4. **Accounting update** — `refunded_amount += client_payout`, `released_amount += freelancer_payout`.
5. **Status update** — `final_status_after_resolution` returns `Refunded` only
   when `refunded_amount == funded_amount`; otherwise `Completed`.
6. **Reputation credit** — if status becomes `Completed`, the freelancer receives
   one pending reputation credit.
7. **Event emission** — `("dispute", "resolved")`.

### Rounding

`PartialRefund` uses **integer floor division** on the freelancer leg:

```
freelancer_payout = floor(available * 30 / 100)
client_payout     = available - freelancer_payout
```

The freelancer leg always rounds **down**, and the client receives the
remainder. This guarantees conservation: `client_payout + freelancer_payout == available`
for any `available ≥ 0`.

The `Split` variant must sum **exactly** to `available` (enforced via
`InvalidDisputeSplit` if `client + freelancer > available` or
`client + freelancer < available`).

### Held funds

Dispute resolution is blocked while paused or in emergency. After resolution,
if the contract is `Refunded`, the accounting reflects that all funded funds
have been allocated. If `Completed`, the freelancer's `released_amount`
reflects their award. No further token transfers occur inside the contract
at resolution time — see [Accounting Guarantees](#5-accounting-guarantees) below.

---

## 5. Protocol Fee Withdrawal (`withdraw_protocol_fees`)

Accrued protocol fees are withdrawn by the admin to a treasury address. This
is the **only** entrypoint that moves funds out of escrow without corresponding
to a milestone or contract lifecycle event.

### Ordering

1. **Preflight validation** — `require_initialized`, pause gate, admin auth.
2. **Amount check** — `amount` must be `> 0` and `≤ AccumulatedProtocolFees`.
3. **SAC transfer: escrow → treasury** — `token.transfer(escrow, to, amount)`.
4. **Accumulated-fees decrement** — `AccumulatedProtocolFees -= amount`.
5. **Event emission** — `("fee", "withdraw")`.

### Rounding

Withdrawn amounts are integer `i128` stroop values with no rounding.

### Held funds

Fee withdrawal is blocked while paused or in emergency.

---

## 6. Accounting Guarantees

Every settlement path preserves the core accounting invariant:

```
funded_amount = released_amount + refunded_amount + available_balance
```

| Path | `funded_amount` | `released_amount` | `refunded_amount` | `AccumulatedProtocolFees` |
|---|---|---|---|---|
| Milestone release | unchanged | `+= net` | unchanged | `+= fee` |
| Cancel contract | unchanged | unchanged | `+= refund` | unchanged |
| Per-milestone refund | unchanged | unchanged | `+= total_refund` | unchanged |
| Dispute resolution | unchanged | `+= freelancer_payout` | `+= client_payout` | unchanged |
| Fee withdrawal | unchanged | unchanged | unchanged | `-= amount` |

The **comprehensive balance invariant** is:

```
escrow SAC balance = released_amount + refunded_amount + AccumulatedProtocolFees + available_balance
```

where `available_balance` is the portion yet to be released, refunded, or
resolved. Protocol fees commingle with the escrow SAC balance until withdrawn.

---

## 7. Worked Numeric Example

### Setup

- Milestones: `[2000, 3000, 5000]` stroops (total = 10_000)
- Protocol fee: 500 bps (5 %)
- Fee on milestone 0: `floor(2000 * 500 / 10_000) = 100`
- Fee on milestone 1: `floor(3000 * 500 / 10_000) = 150`
- Fee on milestone 2: `floor(5000 * 500 / 10_000) = 250`

### Step-by-step

| Step | Action | `funded` | `released` | `refunded` | `acc. fees` | Freelancer received | Client received |
|---|---|---|---|---|---|---|---|
| 0 | Deposit 10_000 | 10_000 | 0 | 0 | 0 | 0 | 0 |
| 1 | Release milestone 0 | 10_000 | 1_900 | 0 | 100 | 1_900 | 0 |
| 2 | Release milestone 1 | 10_000 | 4_750 | 0 | 250 | 4_750 | 0 |
| 3 | Refund milestone 2 | 10_000 | 4_750 | 5_000 | 250 | 4_750 | 5_000 |

After step 3: all milestones terminal (released or refunded), status → `Completed`.
Available balance = `10_000 - 4_750 - 5_000 = 250`, which equals `AccumulatedProtocolFees`.

This is not a coincidence — when all milestones are terminal, the only remaining
funds in escrow are accumulated protocol fees.

### Dispute example — PartialRefund

- Funded: 10_000, released: 3_000, refunded: 1_000, available: 6_000
- Arbiter awards `PartialRefund`:
  - Freelancer: `floor(6_000 * 30 / 100) = 1_800`
  - Client: `6_000 - 1_800 = 4_200`
- After resolution: `released = 3_000 + 1_800 = 4_800`, `refunded = 1_000 + 4_200 = 5_200`
- No tokens move; escrow SAC balance is unchanged.
- Status: `Completed` (since 5_200 ≠ 10_000).

### Dispute example — FullRefund

- Funded: 10_000, released: 0, refunded: 4_000, available: 6_000
- Arbiter awards `FullRefund`:
  - Freelancer: `0`
  - Client: `6_000`
- After resolution: `released = 0, refunded = 4_000 + 6_000 = 10_000`
- Status: `Refunded` (since `refunded_amount == funded_amount`).

---

## 8. Cross-Reference

| Entrypoint | Source Location | Test File |
|---|---|---|
| `release_milestone` | `lib.rs:690` | `test/release.rs` |
| `cancel_contract` | `lib.rs:1593` | `test/cancel_contract.rs` |
| `refund_unreleased_milestones` | `lib.rs:1018` | `test/refund.rs` |
| `raise_dispute` | `lib.rs:2184` | `test/dispute.rs` |
| `resolve_dispute` | `lib.rs:2263` | `test/dispute.rs` |
| `resolution_payouts` | `dispute.rs:30` | `test/dispute.rs` |
| `withdraw_protocol_fees` | `lib.rs:2010` | `test/protocol_fees.rs` |
| Fee calculation | `lib.rs:2120` | `test/protocol_fees.rs` |
