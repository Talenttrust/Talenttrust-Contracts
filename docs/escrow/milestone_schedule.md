# Escrow Milestone Due Dates

Per-milestone due dates are agreed by the client before funding and let the
client reclaim a milestone the freelancer never submitted. The implementation
lives in `contracts/escrow/src/lib.rs`.

`Milestone { amount, released }` is a stored `contracttype`, so its layout is
unchanged. Due dates and reclaim flags live in side entries, exactly like
`DataKey::MilestoneSpecs` / `DataKey::Delivery`:

| Key | Value | Purpose |
|---|---|---|
| `DataKey::MilestoneDue(id)` | `Vec<u64>` | Ledger timestamps, index-aligned with the escrow's milestones |
| `DataKey::MilestoneReclaimed(id, index)` | `bool` | Set once the client reclaimed that milestone |

The side entries are created lazily; an escrow without a `MilestoneDue` entry
has no deadlines and can never have a milestone reclaimed.

## API

### `set_milestone_due(env, id, dates: Vec<u64>)`

The client commits optional deadlines for the escrow's milestones.

- Only while the escrow is `Created` with nothing deposited (same window as
  `set_terms`); afterwards `InvalidStatus`.
- `dates` is either empty (clears any deadlines) or exactly one entry per
  milestone; any other length fails with `InvalidMetadata`.
- Dates must be strictly increasing (`d[i + 1] > d[i]`), else `InvalidMetadata`.
- Gated by the `PAUSE_CREATE` pause category and `require_not_frozen`.
- Emits `("m_due", id) -> dates`.

### `get_milestone_due(env, id, index) -> Option<u64>`

The deadline agreed for a milestone, or `None` when the escrow has no schedule
or the index is out of range.

### `reclaim_overdue(env, id, index)`

The client refunds a milestone whose deadline has passed without a submission.

- Requires `now > due` (`NotExpired` when the deadline has not passed) and a
  configured deadline (`InvalidMetadata` when the milestone has none).
- Rejected while the milestone is submitted (`InvalidStatus`), already released
  or already reclaimed (`MilestoneAlreadyReleased`), and once the escrow is no
  longer `Created` / `Funded` (`InvalidStatus`).
- The milestone's **unpaid** amount (`amount - partially paid`) is refunded
  pro-rata to the escrow's funders (the client when there is no funder record),
  capped at the funds actually held. Funder contributions are reduced with the
  refund so later refunds stay proportional.
- `total_amount` and `funded_amount` shrink by the reclaimed amount, liability
  is reduced by the refund, and an optional outstanding settlement proposal is
  cleared. A partially paid milestone keeps its earlier payment; it can no
  longer be submitted, delivered or released.
- When no milestone is left outstanding the remainder is closed as `Cancelled`
  (any held funds are refunded first) and `ClosedAt` is recorded.
- Gated by the `PAUSE_CANCEL` pause category and `require_not_frozen`; stream
  escrows are rejected (`InvalidStatus`) because they settle only through
  `withdraw_vested` / `stop_stream`.
- Emits `("reclaim", id) -> (index, refund)`.
- `extend_escrow_ttl` keeps both side entries alive.

## Accounting

The core invariants are unchanged:

```text
0 <= released_amount <= funded_amount <= total_amount
```

Reclaiming milestone `i` refunds `min(unpaid_i, funded_amount - released_amount)`
and subtracts `unpaid_i` from `total_amount` and the refunded part from
`funded_amount`, leaving the remainder-to-fund consistent. When every milestone
is released or reclaimed, `funded_amount == released_amount` holds for the
terminal `Cancelled` state.

## Example

```text
create_contract(client, freelancer, token, [100, 200])
set_milestone_due(id, [t0 + 7d, t0 + 14d])   // before any deposit

deposit_funds(id, 300)
// t0 + 7d + 1, milestone 0 still unsubmitted
reclaim_overdue(id, 0)
// -> 100 refunded to the funders
// -> total_amount = 200, funded_amount = 200
```

## Test coverage

`contracts/escrow/src/test.rs` covers setup before/after deposit, length and
monotonicity validation, client-only authorization, the not-yet-due and
submitted rejections, partial payments, pro-rata sponsor refunds, pause /
freeze / stream gating and TTL extension.
