# Escrow State Persistence

This document maps the escrow contract's persisted storage to the lifecycle invariants reviewers should verify.

## Storage Keys

| Key | Value | Purpose |
| --- | --- | --- |
| `PauseAdmin` | `Address` | authority for pause and emergency controls |
| `Paused` | `bool` | fail-closed switch for mutating escrow flows |
| `EmergencyPaused` | `bool` | blocks standard `unpause` until explicit recovery |
| `NextContractId` | `u32` | monotonically increasing escrow identifier counter |
| `Contract(id)` | `EscrowContractData` | full persisted lifecycle and participant record |
| `Reputation(address)` | `ReputationRecord` | aggregate ratings for a freelancer |
| `PendingReputationCredits(address)` | `u32` | count of completed contracts still eligible to issue a rating |
| `GovernanceAdmin` | `Address` | current protocol parameter admin |
| `PendingGovernanceAdmin` | `Address` | proposed next governance admin |
| `ProtocolParameters` | `ProtocolParameters` | live validation bounds for creation and rating |

## Escrow Record Fields

`EscrowContractData` persists:

- `client`
- `freelancer`
- `milestones`
- `milestone_count`
- `total_amount`
- `funded_amount`
- `released_amount`
- `released_milestones`
- `status`
- `reputation_issued`
- `created_at`
- `updated_at`

## Persistence Invariants

Creation invariants:

- `milestone_count == milestones.len()`
- `total_amount == sum(milestones.amount)`
- `funded_amount == 0`
- `released_amount == 0`
- `released_milestones == 0`
- `status == Created`
- `reputation_issued == false`

Funding invariants:

- `0 < funded_amount <= total_amount`
- status becomes `Funded` after the first successful deposit

Release invariants:

- each milestone changes from unreleased to released once
- `released_amount` increases by the released milestone amount
- `released_milestones` increases by one per successful release
- `released_amount <= funded_amount`
- final release transitions `status` to `Completed`

Reputation invariants:

- completed contracts mint one pending reputation credit for the recorded freelancer
- `issue_reputation` consumes exactly one pending credit
- `reputation_issued` is irreversible

## Reviewer Checklist

1. Confirm invalid participant or milestone metadata cannot be persisted.
2. Confirm overfunding is rejected before storage writes.
3. Confirm milestone double release is rejected.
4. Confirm completed contracts can issue reputation once.
5. Confirm pause and emergency flags block every mutating payment path.
