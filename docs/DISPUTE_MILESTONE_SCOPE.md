# Milestone-Scoped Dispute Scope

Dispute state is scoped to one milestone. A disputed milestone pauses only its own release
path; undisputed milestones remain eligible to flow through the escrow lifecycle.

## Overview

When a party opens a dispute using `open_milestone_dispute(id, index)`, only the specific
milestone at the given index is locked. The rest of the escrow continues to operate normally.
This isolation prevents a single disputed milestone from blocking all other milestone releases
or completions.

## Storage

The dispute state is stored under the side-entry data key `DataKey::MilestoneDispute(id, index)`,
which records:

- **opener**: The address that opened the dispute (client or freelancer)
- **remaining**: The remaining unpaid amount of the disputed milestone at dispute open time
- **bond**: The dispute bond posted by the opener, scaled to the milestone's remaining amount

The escrow contract itself remains in its current `ContractStatus` (e.g., `Funded` or
`Disputed` at the escrow level) — only the individual milestone is locked.

## New Entrypoints

### `open_milestone_dispute(env, id, index, caller)`

- **Validates** the escrow and milestone exist, and the milestone is not already released.
- **Collects** a dispute bond scaled to the milestone's remaining amount (not the full escrow total).
- **Records** dispute state using `DataKey::MilestoneDispute(id, index)`.
- **Preserves** the escrow-wide funded state — other milestones can still be released.
- **Emits** `milestone_dispute` event with `(id, index, caller, remaining, bond)`.

**Key behaviors:**
- The escrow stays in its current status; no status transition occurs at the escrow level.
- Only the disputed milestone's remaining amount is locked; other milestones are unaffected.
- The dispute bond is refundable and tracked separately from the held escrow balance.
- If the milestone is already released, the call fails with `MilestoneAlreadyReleased`.

### `resolve_milestone_dispute(env, caller, id, index, freelancer_bps)`

- **Restricts** resolution to the authorized Resolver/admin (or a party holding the `Resolver` role).
- **Splits** only the disputed milestone's remaining amount according to `freelancer_bps`:
  - `to_freelancer = mul_bps(milestone_remaining, freelancer_bps)`
  - `to_client = milestone_remaining - to_freelancer`
- **Settles** the associated bond: returns to the opener if `freelancer_bps >= DISPUTE_WIN_THRESHOLD_BPS`
  (5,000 bps / 50%), otherwise to the counterparty.
- **Updates** milestone outcomes for both parties (incrementing `disputes_won`/`disputes_lost`).
- **Emits** `milestone_resolved` event with `(id, index, freelancer_bps, to_freelancer, to_client, bond, recipient)`.

**Key behaviors:**
- Prevents double resolution — the dispute state is removed after resolution.
- The disputed milestone is marked as released after resolution.
- Other milestones remain in their current state (released or pending).
- If the escrow has other unresolved disputes, they continue independently.

## Independent Milestone Operation

| Scenario | Behavior |
|---|---|
| Release milestone M while milestone N is disputed | ✅ Allowed, if M is otherwise eligible (not released, in-order for sequential escrows) |
| The disputed milestone cannot be released before resolution | ✅ Attempting `release_milestone` on a disputed milestone fails with `MilestoneLockedByDispute` |
| `cancel_contract` cannot refund the disputed milestone's locked amount | ✅ Funds backing the disputed milestone remain in escrow, claimable via `claim_milestone` after review window |
| Other eligible milestones continue to operate | ✅ Unrelated milestones flow normally |
| Completing the final undisputed milestone | ✅ Correctly marks escrow complete, even if another milestone was previously disputed and resolved |
| Resolving the final outstanding dispute | ✅ Correctly updates escrow completion state |

## Accounting Invariants

The following invariants are carefully maintained:

- **Funded amount**: The escrow's `funded_amount` is unchanged by milestone-level disputes.
- **Released amount**: Only the disputed milestone's remaining amount is split; other released amounts are unaffected.
- **Remaining milestone balances**: After dispute resolution, the resolved milestone's `remaining` becomes 0
  (marked `released: true`).
- **Bonds**: The dispute bond is settled independently and is never part of the held balance.
- **Liability**: `get_liability` includes claimable balances from failed payouts, but milestone dispute
  locks do not affect the overall liability calculation.
- **Escrow completion**: The escrow is marked `Completed` only when all milestones are released
  and no unresolved disputes remain.

## Regression Tests

The following test scenarios are covered:

1. **Single milestone dispute**: Open and resolve a dispute on one milestone; verify other milestones
   can still be released.
2. **Multiple milestones, single dispute**: Open disputes on multiple milestones independently; verify
   each can be resolved without affecting the others.
3. **Dispute blocks release**: Verify that attempting to release a disputed milestone fails.
4. **Final milestone completion**: After all milestones are released (some via dispute resolution),
   the escrow correctly transitions to `Completed`.
5. **Cancellation with disputed milestone**: Verify that `cancel_contract` preserves the disputed
   milestone's locked amount and does not refund it to the client.
6. **Authorization failures**: Verify that non-resolvers cannot resolve milestone disputes.
7. **Double resolution prevention**: Verify that resolving an already-resolved milestone fails.
8. **Claimable fallback integration**: Verify that failed payouts during dispute resolution correctly
   credit claimable balances instead of reverting.